import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import { currentLanguage } from "../lib/i18n";
import type { AgentAccess } from "../lib/types";

export function AgentAccessSettings() {
  const en = currentLanguage() === "en";
  const [value, setValue] = useState<AgentAccess | null>(null);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState("");
  useEffect(() => {
    let active = true;
    const reload = () => {
      void api
        .agentAccess()
        .then((v) => {
          if (active) setValue(v);
        })
        .catch(() => {
          if (active) setError(en ? "Could not read Agent permissions" : "无法读取 Agent 权限");
        });
    };
    reload();
    const stop = listen("state-changed", reload);
    return () => {
      active = false;
      void stop.then((unlisten) => unlisten()).catch(() => {});
    };
  }, [en]);
  async function save(next: AgentAccess) {
    setBusy(true);
    setError("");
    try {
      setValue(await api.setAgentAccess(next));
    } catch {
      setError(en ? "Could not save Agent permissions" : "无法保存 Agent 权限");
    } finally {
      setBusy(false);
    }
  }
  return (
    <section className="settings-card">
      <div className="settings-row column">
        <div>
          <strong>{en ? "CLI / AI Agent access" : "CLI / AI Agent 访问"}</strong>
          <span>
            {en
              ? "Allows programs under your Windows user to read account aliases and quotas. Provider CLI login permissions are separate. Refresh runs only in the desktop backend."
              : "允许当前 Windows 用户下的程序读取账号别名和额度。此权限独立于厂商 CLI 登录来源授权；刷新仅由桌面后台执行。"}
          </span>
        </div>
      </div>
      {value &&
        (
          [
            ["enabled", en ? "Allow Agent access" : "允许 Agent 访问"],
            ["allowRefresh", en ? "Allow refresh" : "允许触发刷新"],
            [
              "allowOfflineCache",
              en ? "Allow reading cache when desktop is closed" : "允许桌面关闭时读取离线缓存",
            ],
          ] as const
        ).map(([field, label]) => (
          <label className="settings-row" key={field}>
            <span>{label}</span>
            <input
              type="checkbox"
              checked={value[field]}
              disabled={busy || (field !== "enabled" && !value.enabled)}
              onChange={(event) => void save({ ...value, [field]: event.target.checked })}
            />
          </label>
        ))}
      {error && <p role="alert">{error}</p>}
    </section>
  );
}
