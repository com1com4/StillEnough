import type { NetworkConfig } from "../lib/types";

export function NetworkFields({
  value,
  onChange,
  global = false,
}: {
  value: NetworkConfig;
  onChange: (value: NetworkConfig) => void;
  global?: boolean;
}) {
  return (
    <>
      <label>
        网络方式
        <select
          value={value.mode}
          onChange={(e) => onChange({ mode: e.target.value as NetworkConfig["mode"], proxyUrl: "" })}
        >
          {!global && <option value="inherit">使用全局设置</option>}
          <option value="system">跟随系统</option>
          <option value="direct">直连</option>
          <option value="custom">自定义代理</option>
        </select>
      </label>
      {value.mode === "custom" && (
        <label>
          代理地址
          <input
            value={value.proxyUrl}
            maxLength={512}
            placeholder="http://127.0.0.1:7890"
            autoComplete="off"
            onChange={(e) => onChange({ ...value, proxyUrl: e.target.value.trim() })}
          />
          <span className="settings-hint">
            支持 HTTP、SOCKS5（本地 DNS）、SOCKS5h（代理 DNS）；暂不支持账号密码。
          </span>
        </label>
      )}
      {value.mode === "system" && (
        <p className="settings-hint">
          优先当前进程代理环境变量，其次 Windows 静态系统代理。PAC
          暂不支持；环境变量变更需重启应用。外部静态代理变更后请重新保存。
        </p>
      )}
    </>
  );
}
