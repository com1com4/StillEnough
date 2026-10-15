use reqwest::{Client, Proxy};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Mutex, time::Duration};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct NetworkConfig {
    pub mode: String,
    #[serde(default)]
    pub proxy_url: String,
}
impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            mode: "inherit".into(),
            proxy_url: String::new(),
        }
    }
}
impl NetworkConfig {
    pub fn system() -> Self {
        Self {
            mode: "system".into(),
            proxy_url: String::new(),
        }
    }
    pub fn validate(&self, global: bool) -> Result<(), String> {
        if self.proxy_url.len() > 512
            || self
                .proxy_url
                .chars()
                .any(|c| c.is_whitespace() || c.is_control())
        {
            return Err("代理地址过长或包含空白字符".into());
        }
        match self.mode.as_str() {
            "inherit" if !global => {}
            "system" | "direct" => {}
            "custom" => {
                let url = reqwest::Url::parse(&self.proxy_url).map_err(|_| "请输入有效代理地址")?;
                if !["http", "socks5", "socks5h"].contains(&url.scheme())
                    || url.host_str().is_none()
                    || !url.username().is_empty()
                    || url.password().is_some()
                    || url.query().is_some()
                    || url.fragment().is_some()
                    || url.path() != "/" && !url.path().is_empty()
                    || !has_explicit_port(&self.proxy_url)
                {
                    return Err(
                        "代理仅支持 http/socks5/socks5h://主机:端口，不支持认证、路径或参数".into(),
                    );
                }
                return Ok(());
            }
            _ => return Err("网络配置无效，请重新保存查询网络设置".into()),
        }
        if !self.proxy_url.is_empty() {
            return Err("非自定义模式不能包含代理地址".into());
        }
        Ok(())
    }
}
fn has_explicit_port(value: &str) -> bool {
    value
        .split_once("://")
        .and_then(|(_, authority)| authority.trim_end_matches('/').rsplit_once(':'))
        .and_then(|(_, port)| port.parse::<u16>().ok())
        .is_some_and(|port| port > 0)
}
pub fn build(config: &NetworkConfig) -> Result<Client, String> {
    config.validate(true)?;
    if config.mode == "system" {
        check_system_proxy()?;
    }
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .connect_timeout(Duration::from_secs(8));
    match config.mode.as_str() {
        "direct" => builder = builder.no_proxy(),
        "custom" => {
            builder = builder
                .no_proxy()
                .proxy(Proxy::all(&config.proxy_url).map_err(|_| "代理地址无效")?)
        }
        _ => {}
    }
    builder.build().map_err(|_| "网络客户端初始化失败".into())
}
#[cfg(windows)]
fn check_system_proxy() -> Result<(), String> {
    // Do not execute PAC scripts or claim that a PAC-only route has been applied.
    if [
        "HTTP_PROXY",
        "http_proxy",
        "HTTPS_PROXY",
        "https_proxy",
        "ALL_PROXY",
        "all_proxy",
    ]
    .iter()
    .any(|name| std::env::var(name).is_ok_and(|v| !v.is_empty()))
    {
        return Ok(());
    }
    use winreg::{enums::HKEY_CURRENT_USER, RegKey};
    if let Ok(key) = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Internet Settings")
    {
        let pac: String = key.get_value("AutoConfigURL").unwrap_or_default();
        let enabled: u32 = key.get_value("ProxyEnable").unwrap_or_default();
        if !pac.is_empty() && enabled == 0 {
            return Err("系统仅配置 PAC 代理，暂不支持；请指定 HTTP 或 SOCKS 代理".into());
        }
    }
    Ok(())
}
#[cfg(not(windows))]
fn check_system_proxy() -> Result<(), String> {
    Ok(())
}
struct State {
    config: Result<NetworkConfig, String>,
    revision: u64,
    clients: HashMap<NetworkConfig, Client>,
}
pub struct Network {
    state: Mutex<State>,
}
impl Network {
    pub fn new() -> Self {
        Self {
            state: Mutex::new(State {
                config: Ok(NetworkConfig::system()),
                revision: 0,
                clients: HashMap::new(),
            }),
        }
    }
    pub fn load(&self, db: &rusqlite::Connection) {
        use rusqlite::OptionalExtension;
        let loaded = db
            .query_row(
                "SELECT value_json FROM app_settings WHERE key='network'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional();
        let config = match loaded {
            Ok(None) => Ok(NetworkConfig::system()),
            Ok(Some(json)) => serde_json::from_str::<NetworkConfig>(&json)
                .map_err(|_| "网络设置损坏，请重新保存".to_string())
                .and_then(|c| {
                    c.validate(true)?;
                    Ok(c)
                }),
            Err(_) => Err("网络设置读取失败".into()),
        };
        if let Ok(mut state) = self.state.lock() {
            state.config = config;
            state.clients.clear();
        }
    }
    pub fn config(&self) -> Result<NetworkConfig, String> {
        self.state
            .lock()
            .map_err(|_| "网络设置不可用")?
            .config
            .clone()
    }
    pub fn revision(&self) -> u64 {
        self.state.lock().map(|s| s.revision).unwrap_or(u64::MAX)
    }
    pub fn save(&self, db: &rusqlite::Connection, config: NetworkConfig) -> Result<(), String> {
        let client = build(&config)?;
        let mut state = self.state.lock().map_err(|_| "网络设置不可用")?;
        db.execute("INSERT INTO app_settings(key,value_json,updated_at) VALUES('network',?1,datetime('now')) ON CONFLICT(key) DO UPDATE SET value_json=excluded.value_json,updated_at=excluded.updated_at",
            [serde_json::to_string(&config).map_err(|_| "网络设置无效")?]).map_err(|_| "网络设置保存失败")?;
        state.clients.clear();
        state.clients.insert(config.clone(), client);
        state.config = Ok(config);
        state.revision += 1;
        Ok(())
    }
    pub fn resolve(&self, account: &NetworkConfig) -> Result<NetworkConfig, String> {
        account.validate(false)?;
        if account.mode == "inherit" {
            self.config()
        } else {
            Ok(account.clone())
        }
    }
    pub fn client(&self, config: &NetworkConfig) -> Result<Client, String> {
        let mut state = self.state.lock().map_err(|_| "网络设置不可用")?;
        if let Some(client) = state.clients.get(config) {
            return Ok(client.clone());
        }
        let client = build(config)?;
        if state.clients.len() >= 32 {
            state.clients.clear();
        }
        state.clients.insert(config.clone(), client.clone());
        Ok(client)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn custom(url: &str) -> NetworkConfig {
        NetworkConfig {
            mode: "custom".into(),
            proxy_url: url.into(),
        }
    }
    #[test]
    fn rejects_credentials_paths_and_invalid_ports() {
        for url in [
            "http://user:secret@localhost:8080",
            "http://localhost",
            "http://localhost:0",
            "http://localhost:65536",
            "http://localhost:80/path",
            "http://localhost:80?q=x",
            "https://localhost:80",
            "socks5://localhost:80#x",
        ] {
            assert!(custom(url).validate(true).is_err());
        }
        for url in [
            "http://localhost:80",
            "socks5://127.0.0.1:1080",
            "socks5h://[::1]:1080",
        ] {
            assert!(custom(url).validate(true).is_ok());
        }
    }
    #[test]
    fn bad_stored_network_does_not_fall_back_to_direct() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch("CREATE TABLE app_settings(key TEXT PRIMARY KEY,value_json TEXT,updated_at TEXT); INSERT INTO app_settings VALUES('network','broken','now');").unwrap();
        let network = Network::new();
        network.load(&db);
        assert!(network.resolve(&NetworkConfig::default()).is_err());
        let direct = NetworkConfig {
            mode: "direct".into(),
            proxy_url: String::new(),
        };
        assert_eq!(network.resolve(&direct).unwrap(), direct);
        network.save(&db, direct.clone()).unwrap();
        assert_eq!(network.config().unwrap(), direct);
        assert_eq!(network.revision(), 1);
    }
    #[tokio::test]
    async fn http_proxy_receives_absolute_target_and_connect() {
        use std::io::{Read, Write};
        for (target, expected) in [
            (
                "http://unresolvable.invalid/balance",
                "GET http://unresolvable.invalid/balance HTTP/1.1",
            ),
            (
                "https://unresolvable.invalid/balance",
                "CONNECT unresolvable.invalid:443 HTTP/1.1",
            ),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = [0; 2048];
                let n = socket.read(&mut bytes).unwrap();
                assert!(n > 0);
                let request = String::from_utf8_lossy(&bytes[..n]).to_string();
                socket.write_all(b"HTTP/1.1 502 Bad Gateway\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").unwrap();
                request
            });
            let response = build(&custom(&format!("http://{address}")))
                .unwrap()
                .get(target)
                .send()
                .await;
            if target.starts_with("https") {
                assert!(response.is_err());
            } else {
                assert_eq!(response.unwrap().status(), 502);
            }
            assert!(task.join().unwrap().starts_with(expected));
        }
    }
    #[tokio::test]
    async fn socks_dns_direction_is_explicit() {
        use std::io::{Read, Write};
        for (scheme, target, expected_type) in [
            ("socks5", "http://127.0.0.1:12345/", 1),
            ("socks5h", "http://unresolvable.invalid/", 3),
        ] {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = listener.local_addr().unwrap();
            let task = std::thread::spawn(move || {
                let (mut socket, _) = listener.accept().unwrap();
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut hello = [0; 3];
                socket.read_exact(&mut hello).unwrap();
                assert_eq!(hello, [5, 1, 0]);
                socket.write_all(&[5, 0]).unwrap();
                let mut command = [0; 4];
                socket.read_exact(&mut command).unwrap();
                let len = match command[3] {
                    1 => 4,
                    3 => {
                        let mut n = [0];
                        socket.read_exact(&mut n).unwrap();
                        n[0] as usize
                    }
                    _ => panic!("unexpected address family"),
                };
                let mut destination = vec![0; len + 2];
                socket.read_exact(&mut destination).unwrap();
                socket.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]).unwrap();
                (command[3], destination)
            });
            assert!(build(&custom(&format!("{scheme}://{address}")))
                .unwrap()
                .get(target)
                .send()
                .await
                .is_err());
            let (kind, destination) = task.join().unwrap();
            assert_eq!(kind, expected_type);
            if kind == 3 {
                assert_eq!(
                    &destination[..destination.len() - 2],
                    b"unresolvable.invalid"
                );
            }
        }
    }
    #[tokio::test]
    async fn custom_failure_never_reaches_direct_destination() {
        let target = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        target.set_nonblocking(true).unwrap();
        let proxy = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy_address = proxy.local_addr().unwrap();
        drop(proxy);
        let network = Network::new();
        let config = custom(&format!("http://{proxy_address}"));
        assert!(network
            .client(&config)
            .unwrap()
            .get(format!("http://{}/", target.local_addr().unwrap()))
            .send()
            .await
            .is_err());
        assert_eq!(
            target.accept().unwrap_err().kind(),
            std::io::ErrorKind::WouldBlock
        );
    }
    #[tokio::test]
    async fn account_override_and_failed_save_preserve_route() {
        let db = rusqlite::Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE app_settings(key TEXT PRIMARY KEY,value_json TEXT,updated_at TEXT);",
        )
        .unwrap();
        let network = Network::new();
        let config = custom("http://127.0.0.1:7890");
        network.save(&db, config.clone()).unwrap();
        assert_eq!(network.resolve(&NetworkConfig::default()).unwrap(), config);
        let direct = NetworkConfig {
            mode: "direct".into(),
            proxy_url: String::new(),
        };
        assert_eq!(network.resolve(&direct).unwrap(), direct);
        db.execute_batch("DROP TABLE app_settings").unwrap();
        assert!(network.save(&db, direct).is_err());
        assert_eq!(network.config().unwrap(), config);
        assert_eq!(network.revision(), 1);
    }
    #[test]
    fn system_environment_priority_and_bypass_are_isolated_in_child() {
        use std::io::{Read, Write};
        fn server(body: &'static str) -> (String, std::thread::JoinHandle<()>) {
            let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            let address = format!("http://{}", listener.local_addr().unwrap());
            let task = std::thread::spawn(move || {
                listener.set_nonblocking(true).unwrap();
                let deadline = std::time::Instant::now() + Duration::from_secs(20);
                let mut socket = loop {
                    match listener.accept() {
                        Ok((socket, _)) => break socket,
                        Err(e)
                            if e.kind() == std::io::ErrorKind::WouldBlock
                                && std::time::Instant::now() < deadline =>
                        {
                            std::thread::sleep(Duration::from_millis(10))
                        }
                        Err(e) => panic!("test server did not receive request: {e}"),
                    }
                };
                socket
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                let mut bytes = [0; 2048];
                let n = socket.read(&mut bytes).unwrap();
                assert!(n > 0);
                socket.write_all(format!("HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}", body.len()).as_bytes()).unwrap();
            });
            (address, task)
        }
        let (proxy, proxy_task) = server("proxy");
        let (direct, direct_task) = server("direct");
        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "services::network::tests::environment_worker",
            "--nocapture",
        ]);
        for name in [
            "HTTP_PROXY",
            "HTTPS_PROXY",
            "ALL_PROXY",
            "NO_PROXY",
            "http_proxy",
            "https_proxy",
            "all_proxy",
            "no_proxy",
        ] {
            command.env_remove(name);
        }
        let status = command
            .env("HTTP_PROXY", proxy)
            .env("ALL_PROXY", "http://127.0.0.1:1")
            .env("NO_PROXY", "127.0.0.1")
            .env("STILLENOUGH_NETWORK_TEST_TARGET", direct)
            .status()
            .unwrap();
        proxy_task.join().unwrap();
        direct_task.join().unwrap();
        assert!(status.success());
    }
    #[tokio::test]
    async fn environment_worker() {
        let Ok(target) = std::env::var("STILLENOUGH_NETWORK_TEST_TARGET") else {
            return;
        };
        let client = build(&NetworkConfig::system()).unwrap();
        assert_eq!(
            client
                .get("http://unresolvable.invalid/balance")
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "proxy"
        );
        assert_eq!(
            client
                .get(target)
                .send()
                .await
                .unwrap()
                .text()
                .await
                .unwrap(),
            "direct"
        );
    }
}
