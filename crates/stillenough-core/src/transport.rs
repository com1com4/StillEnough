use crate::{protocol::Response, request::Request};
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
pub const MAX_REQUEST: usize = 1024 * 1024;
pub const MAX_RESPONSE: usize = 16 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Reply {
    pub protocol_major: u8,
    pub protocol_minor: u8,
    pub schema_version: u8,
    pub app_version: String,
    pub instance_id: String,
    pub response: Response,
}
pub async fn read_frame<R: AsyncRead + Unpin>(
    reader: &mut R,
    max: usize,
) -> Result<Vec<u8>, &'static str> {
    let size = reader
        .read_u32_le()
        .await
        .map_err(|_| "desktop_unavailable")? as usize;
    if size == 0 || size > max {
        return Err("version_mismatch");
    }
    let mut bytes = vec![0; size];
    reader
        .read_exact(&mut bytes)
        .await
        .map_err(|_| "desktop_unavailable")?;
    Ok(bytes)
}
pub async fn write_frame<W: AsyncWrite + Unpin, T: Serialize>(
    writer: &mut W,
    value: &T,
    max: usize,
) -> Result<(), &'static str> {
    let bytes = serde_json::to_vec(value).map_err(|_| "internal_error")?;
    if bytes.len() > max {
        return Err("version_mismatch");
    }
    writer
        .write_u32_le(bytes.len() as u32)
        .await
        .map_err(|_| "desktop_unavailable")?;
    writer
        .write_all(&bytes)
        .await
        .map_err(|_| "desktop_unavailable")?;
    writer.flush().await.map_err(|_| "desktop_unavailable")?;
    Ok(())
}
#[cfg(windows)]
pub fn create_server(
    path: &std::path::Path,
    first: bool,
) -> Result<tokio::net::windows::named_pipe::NamedPipeServer, &'static str> {
    let security = crate::windows::Security::new()?;
    let mut attrs = security.attributes();
    let mut options = tokio::net::windows::named_pipe::ServerOptions::new();
    options
        .first_pipe_instance(first)
        .reject_remote_clients(true)
        .max_instances(16);
    unsafe {
        options.create_with_security_attributes_raw(
            crate::windows::endpoint(path)?,
            (&mut attrs as *mut windows_sys::Win32::Security::SECURITY_ATTRIBUTES).cast(),
        )
    }
    .map_err(|_| "owner_conflict")
}
#[cfg(windows)]
pub async fn query(path: &std::path::Path, request: &Request) -> Result<Response, &'static str> {
    use tokio::net::windows::named_pipe::ClientOptions;
    let endpoint = crate::windows::endpoint(path)?;
    let deadline = chrono::DateTime::parse_from_rfc3339(&request.deadline)
        .map_err(|_| "invalid_arguments")?
        .signed_duration_since(chrono::Utc::now())
        .to_std()
        .map_err(|_| "deadline_exceeded")?;
    tokio::time::timeout(deadline + std::time::Duration::from_millis(300), async {
        let mut pipe = loop {
            match ClientOptions::new().open(&endpoint) {
                Ok(pipe) => break pipe,
                Err(e) if e.raw_os_error() == Some(231) => {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await
                }
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    return Err("desktop_unavailable")
                }
                Err(_) => return Err("agent_access_denied"),
            }
        };
        crate::windows::verify_peer(&pipe, true)?;
        write_frame(&mut pipe, request, MAX_REQUEST)
            .await
            .map_err(|_| "network_error")?;
        let reply: Reply =
            serde_json::from_slice(&read_frame(&mut pipe, MAX_RESPONSE).await.map_err(|c| {
                if c == "desktop_unavailable" {
                    "network_error"
                } else {
                    c
                }
            })?)
            .map_err(|_| "version_mismatch")?;
        if reply.protocol_major != 1
            || reply.schema_version != 1
            || reply.response.schema_version != 1
            || reply.response.request_id != request.request_id
            || reply.response.instance_id.as_deref() != Some(reply.instance_id.as_str())
        {
            return Err("version_mismatch");
        }
        let _ = pipe.write_u8(1).await;
        Ok(reply.response)
    })
    .await
    .map_err(|_| "deadline_exceeded")?
}
#[cfg(not(windows))]
pub async fn query(_: &std::path::Path, _: &Request) -> Result<Response, &'static str> {
    Err("unsupported")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn length_limits_truncation_and_unicode() {
        let (mut a, mut b) = tokio::io::duplex(100);
        a.write_u32_le(u32::MAX).await.unwrap();
        assert_eq!(
            read_frame(&mut b, MAX_REQUEST).await.unwrap_err(),
            "version_mismatch"
        );
        let (mut a, mut b) = tokio::io::duplex(100);
        a.write_u32_le(5).await.unwrap();
        a.write_all(b"ab").await.unwrap();
        drop(a);
        assert!(read_frame(&mut b, 10).await.is_err());
        let (mut a, mut b) = tokio::io::duplex(100);
        write_frame(&mut a, &"中文😀", 100).await.unwrap();
        let value: String =
            serde_json::from_slice(&read_frame(&mut b, 100).await.unwrap()).unwrap();
        assert_eq!(value, "中文😀");
    }
    #[cfg(windows)]
    #[tokio::test]
    async fn windows_pipe_identity_first_instance_and_protocol_mismatch() {
        use std::path::PathBuf;
        let path = PathBuf::from(format!("test-{}", uuid::Uuid::new_v4()));
        let mut server = create_server(&path, true).unwrap();
        assert_eq!(create_server(&path, true).unwrap_err(), "owner_conflict");
        let request = Request::new(crate::request::Command::Quota, 2);
        let id = request.request_id.clone();
        let task = tokio::spawn(async move {
            server.connect().await.unwrap();
            crate::windows::verify_peer(&server, false).unwrap();
            let incoming: Request =
                serde_json::from_slice(&read_frame(&mut server, MAX_REQUEST).await.unwrap())
                    .unwrap();
            assert_eq!(incoming.request_id, id);
            let mut r = Response::local("quota", "snapshot");
            r.request_id = id;
            r.instance_id = Some("test".into());
            let reply = Reply {
                protocol_major: 99,
                protocol_minor: 0,
                schema_version: 1,
                app_version: "test".into(),
                instance_id: "test".into(),
                response: r,
            };
            write_frame(&mut server, &reply, MAX_RESPONSE)
                .await
                .unwrap();
        });
        assert_eq!(
            query(&path, &request).await.unwrap_err(),
            "version_mismatch"
        );
        task.await.unwrap();
    }
    #[cfg(windows)]
    #[test]
    fn owner_lock_is_unique_and_released() {
        let path = std::path::PathBuf::from(format!("lock-{}", uuid::Uuid::new_v4()));
        let first = crate::windows::owner_lock(&path).unwrap();
        assert!(crate::windows::owner_lock(&path).is_err());
        drop(first);
        assert!(crate::windows::owner_lock(&path).is_ok());
    }
}
