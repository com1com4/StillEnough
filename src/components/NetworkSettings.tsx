import { useEffect, useState } from "react";
import { api } from "../lib/ipc";
import type { Account, NetworkConfig } from "../lib/types";
import { fetchStatusLabel } from "../lib/presentation";
import { NetworkFields } from "./NetworkFields";

export function NetworkSettings({ accounts }: { accounts: Account[] }) {
  const [value, setValue] = useState<NetworkConfig>({ mode: "system", proxyUrl: "" });
  const [accountId, setAccountId] = useState("");
  const [busy, setBusy] = useState(false);
  const [ready, setReady] = useState(false);
  const [message, setMessage] = useState("");
  useEffect(() => {
    let active = true;
    void api
      .networkSettings()
      .then((v) => {
        if (active) setValue(v);
      })
      .catch(() => {
        if (active) setMessage("网络设置读取失败；请重新选择并保存，修复前继承全局设置的查询会失败。");
      })
      .finally(() => {
        if (active) setReady(true);
      });
    return () => {
      active = false;
    };
  }, []);
  async function save() {
    setBusy(true);
    setMessage("");
    try {
      await api.saveNetworkSettings(value);
      setMessage("已保存，从下一次查询生效。在途请求继续使用原设置。");
    } catch (error) {
      setMessage(String(error));
    } finally {
      setBusy(false);
    }
  }
  async function test() {
    const account = accounts.find((a) => a.id === accountId);
    if (!account) return;
    setBusy(true);
    setMessage("");
    let session: string | undefined;
    try {
      session = await api.beginSetup({
        id: account.id,
        providerId: account.providerId,
        alias: account.alias,
        baseUrl: account.baseUrl ?? "",
        enabled: account.enabled,
        cliAuthorized: account.cliAuthorized,
        network: value,
      });
      await api.probe(session);
      setMessage(
        account.baseUrl === "cli://bailian/token-plan"
          ? "官方 CLI 成功返回额度；其代理由 CLI 自身管理，本测试不证明应用代理生效。"
          : "通过当前网络草稿成功读取该账号额度/余额；尚未保存网络设置。",
      );
    } catch (error) {
      const code = String(error);
      setMessage(
        ["auth_required", "permission_denied", "rate_limited"].includes(code)
          ? `未读取到额度：${fetchStatusLabel(code)}。本地凭据缺失或冷却也可能导致此结果，不能据此确认本次网络连通。`
          : fetchStatusLabel(code),
      );
    } finally {
      if (session) await api.cancelSetup(session).catch(() => {});
      setBusy(false);
    }
  }
  return (
    <section className="settings-card network-settings">
      <div className="settings-row column">
        <strong>查询网络</strong>
        <span>账号测试与所有刷新共用此设置；账号可单独覆盖。代理失败不会自动转直连。</span>
        <fieldset disabled={busy || !ready} className="connection-fields">
          <NetworkFields
            value={value}
            global
            onChange={(v) => {
              setValue(v);
              setMessage("");
            }}
          />
          <label>
            测试账号（使用当前网络草稿）
            <select
              value={accountId}
              onChange={(e) => {
                setAccountId(e.target.value);
                setMessage("");
              }}
            >
              <option value="">选择已有账号</option>
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {a.productName} · {a.alias}
                </option>
              ))}
            </select>
          </label>
          {!accounts.length && <p className="settings-hint">添加账号后可测试。保存设置无需先测试。</p>}
          <div className="settings-inline">
            <button
              type="button"
              className="button button-quiet"
              onClick={() => void test()}
              disabled={!accountId}
            >
              测试读取
            </button>
            <button type="button" className="button button-primary" onClick={() => void save()}>
              保存网络设置
            </button>
          </div>
        </fieldset>
        <span className="settings-hint">
          百炼 CLI 仅沿用自身环境；Codex app-server 支持自定义 HTTP 代理，暂不支持指定 SOCKS。Codex HTTP 与
          Claude 使用应用统一代理。
        </span>
        {message && <p role="status">{message}</p>}
      </div>
    </section>
  );
}
