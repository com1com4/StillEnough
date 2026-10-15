import { NetworkFields } from "./NetworkFields";
import { useEffect, useRef, useState } from "react";
import type { AccountInput, ProbeResult, ProviderDescriptor } from "../lib/types";
import { api } from "../lib/ipc";
import { balanceText, balanceLabel, fetchStatusLabel, metricLabel } from "../lib/presentation";
import { XIcon } from "@phosphor-icons/react";

export function ConnectionEditor({
  value,
  onChange,
  onClose,
  onSaved,
}: {
  value: AccountInput;
  onChange: (value: AccountInput) => void;
  onClose: () => void;
  onSaved: () => void;
}) {
  const [providers, setProviders] = useState<ProviderDescriptor[]>([]);
  const [error, setError] = useState("");
  const [busy, setBusy] = useState(false);
  const [probe, setProbe] = useState<ProbeResult | null>(null);
  const secret = useRef<HTMLInputElement>(null);
  const accessKeyId = useRef<HTMLInputElement>(null);
  const feedback = useRef<HTMLDivElement>(null);
  const session = useRef<string | null>(null);
  const mounted = useRef(true);
  const selected = providers.find((p) => p.id === value.providerId);
  // Temporarily hide app-server from new connections; existing accounts remain manageable.
  const selectableProviders = providers.filter((p) => p.id !== "codex" || value.id !== null);
  const isCli = ["cli", "local_oauth"].includes(selected?.authMethod ?? "");
  const credentialLabel = selected?.authMethod === "access_key_pair" ? "AccessKey Secret" : "API Key";
  useEffect(() => {
    if (busy || error || probe) feedback.current?.scrollIntoView({ block: "nearest" });
  }, [busy, error, probe]);
  useEffect(() => {
    mounted.current = true;
    void api
      .providers()
      .then(setProviders)
      .catch((reason) => setError(String(reason)));
    return () => {
      mounted.current = false;
      if (session.current) void api.cancelSetup(session.current).catch(() => {});
    };
  }, []);
  function invalidate() {
    if (session.current) void api.cancelSetup(session.current).catch(() => {});
    session.current = null;
    setProbe(null);
  }
  function update(patch: Partial<AccountInput>) {
    if (
      patch.providerId !== undefined ||
      patch.baseUrl !== undefined ||
      patch.cliAuthorized !== undefined ||
      patch.network !== undefined
    )
      invalidate();
    if (patch.providerId !== undefined) {
      if (secret.current) secret.current.value = "";
      if (accessKeyId.current) accessKeyId.current.value = "";
    }
    setError("");
    onChange({ ...value, ...patch });
  }
  async function prepare(requireCliAuthorization = true) {
    if (!selected?.supported) throw new Error("所选产品尚未支持读取");
    if (!value.alias.trim()) throw new Error("请填写账号别名");
    if (isCli && requireCliAuthorization && !value.cliAuthorized)
      throw new Error("请先勾选授权本地登录来源查询");
    if (!session.current) {
      const id = await api.beginSetup(value);
      if (!mounted.current) {
        void api.cancelSetup(id);
        throw new Error("配置已关闭");
      }
      session.current = id;
    }
    if (!isCli && secret.current?.value) {
      if (selected.authMethod === "access_key_pair" && !accessKeyId.current?.value.trim())
        throw new Error("请填写 AccessKey ID");
      const credential =
        selected.authMethod === "access_key_pair"
          ? JSON.stringify({
              accessKeyId: accessKeyId.current?.value.trim(),
              accessKeySecret: secret.current.value,
            })
          : secret.current.value;
      const request = api.setCredential(session.current, credential);
      secret.current.value = "";
      if (accessKeyId.current) accessKeyId.current.value = "";
      await request;
    }
    return session.current;
  }
  async function test() {
    setBusy(true);
    setError("");
    setProbe(null);
    try {
      const id = await prepare();
      const result = await api.probe(id);
      if (mounted.current) setProbe(result);
    } catch (reason) {
      if (mounted.current)
        setError(fetchStatusLabel(reason instanceof Error ? reason.message : String(reason)));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  async function save() {
    if (!value.id && (!value.enabled || !probe)) return;
    setBusy(true);
    setError("");
    try {
      const id = await prepare(value.enabled || !value.id);
      await api.saveConnection(id, value.alias, value.enabled);
      session.current = null;
      if (mounted.current) onSaved();
    } catch (reason) {
      if (mounted.current)
        setError(fetchStatusLabel(reason instanceof Error ? reason.message : String(reason)));
    } finally {
      if (mounted.current) setBusy(false);
    }
  }
  return (
    <div className="modal-backdrop">
      <section className="editor-modal" role="dialog" aria-modal="true" aria-labelledby="connection-heading">
        <header className="modal-header">
          <div>
            <span className="eyebrow">ACCOUNT CONNECTION</span>
            <h2 id="connection-heading">{value.id ? "管理连接" : "添加账号"}</h2>
          </div>
          <button className="icon-button" onClick={onClose} aria-label="关闭">
            <XIcon />
          </button>
        </header>
        <form
          onSubmit={(e) => {
            e.preventDefault();
            void save();
          }}
        >
          <div className="modal-scroll">
            <p className="connection-flow">选择产品 → 配置连接 → 测试读取 → 保存账号</p>
            <fieldset disabled={busy} className="connection-fields">
              <label>
                产品
                <select
                  required
                  value={value.providerId}
                  disabled={!!value.id}
                  onChange={(e) => {
                    const p = providers.find((p) => p.id === e.target.value);
                    update({
                      providerId: e.target.value,
                      baseUrl: p?.baseUrls[0] ?? "",
                      enabled: false,
                      cliAuthorized: false,
                    });
                  }}
                >
                  <option value="">选择要监控的产品</option>
                  {selectableProviders.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.name}
                      {!p.supported ? ` · ${p.status}` : p.name.startsWith("阿里") ? " · 实验性" : ""}
                    </option>
                  ))}
                </select>
              </label>
              {selected && (
                <details className="connection-capability connection-notes">
                  <summary>
                    {selected.capability}
                    <span>查看说明</span>
                  </summary>
                  <p>{selected.limitation}</p>
                </details>
              )}
              {selected?.supported && (
                <>
                  <label>
                    账号别名
                    <input
                      required
                      maxLength={80}
                      value={value.alias}
                      onChange={(e) => update({ alias: e.target.value })}
                      placeholder="例如：个人 API 账号"
                    />
                  </label>
                  <details>
                    <summary>网络方式</summary>
                    <NetworkFields
                      value={value.network ?? { mode: "inherit", proxyUrl: "" }}
                      onChange={(network) => update({ network })}
                    />
                    {isCli && <p className="settings-hint">{selected.networkNote}</p>}
                  </details>
                  {isCli ? (
                    <div className="connection-capability">
                      <strong>通过已有本地登录查询</strong>
                      <pre>{selected.setupInstructions}</pre>
                      <p>
                        仅使用此产品当前本地登录来源。切换账号后请重新测试；同一来源只能保存一个监控账号。
                      </p>
                      <label className="connection-enable">
                        <input
                          type="checkbox"
                          checked={value.cliAuthorized ?? false}
                          onChange={(e) => update({ cliAuthorized: e.target.checked, enabled: false })}
                        />
                        <span>{selected.authorizationLabel}</span>
                      </label>
                    </div>
                  ) : (
                    <>
                      <label>
                        API 服务地址
                        <select value={value.baseUrl} onChange={(e) => update({ baseUrl: e.target.value })}>
                          {selected.baseUrls.map((url) => (
                            <option key={url} value={url}>
                              {url}
                            </option>
                          ))}
                        </select>
                      </label>
                      {selected.authMethod === "access_key_pair" && (
                        <label>
                          AccessKey ID
                          <input
                            ref={accessKeyId}
                            type="password"
                            autoComplete="off"
                            spellCheck={false}
                            onChange={invalidate}
                            placeholder="输入 AccessKey ID"
                          />
                        </label>
                      )}
                      <label>
                        {value.id ? `替换 ${credentialLabel}（留空保留已配置凭据）` : credentialLabel}
                        <input
                          ref={secret}
                          type="password"
                          autoComplete="off"
                          spellCheck={false}
                          onChange={invalidate}
                          placeholder={value.id ? "已配置 · 不回显 Key" : "输入只读查询所需的 Key"}
                        />
                      </label>
                      <p className="privacy-note">
                        凭据保存在当前 Windows 用户的凭据管理器中，不写入本地数据库。 测试提交后输入会清空。
                      </p>
                    </>
                  )}
                  <label className="connection-enable">
                    <input
                      type="checkbox"
                      checked={value.enabled}
                      onChange={(e) => update({ enabled: e.target.checked })}
                    />
                    {isCli
                      ? "启用运行期间的额度自动刷新"
                      : value.id
                        ? "自动刷新额度与余额"
                        : "我同意应用保存凭据，并定期查询该账号的额度或余额"}
                  </label>
                  <p className="privacy-note">
                    {isCli
                      ? "测试读取成功并确认当前登录账号后才能添加。取消授权后可保存停用连接。"
                      : "测试读取成功并确认后才能添加账号。替换凭据会清除该连接的旧观测与历史。"}
                  </p>
                  <button
                    type="button"
                    className="button button-quiet"
                    onClick={() => void test()}
                    disabled={busy || (isCli && !value.cliAuthorized)}
                  >
                    {busy ? "处理中…" : "测试连接"}
                  </button>
                </>
              )}
            </fieldset>
            <div ref={feedback} aria-live="polite" aria-busy={busy}>
              {busy && (
                <p className="privacy-note" role="status">
                  正在处理，请稍候…
                </p>
              )}
              {error && (
                <p className="connection-error" role="alert">
                  {error}
                </p>
              )}
              {probe && (
                <div className="connection-result" role="status">
                  <strong>
                    {isCli
                      ? "已读取当前本地登录账号额度 · 身份未核验"
                      : "读取成功 · 按配置凭据绑定，身份未核验"}
                  </strong>
                  {probe.metrics.map((m) => (
                    <p key={m.metricId}>
                      {m.kind === "balance" ? balanceLabel(m) : metricLabel(m.label)}：
                      {(m.kind === "balance" ? balanceText(m) : m.remaining) ??
                        (m.remainingPercent != null
                          ? `剩余 ${Math.round(m.remainingPercent * 10) / 10}%`
                          : "未知 / 未提供")}
                    </p>
                  ))}
                  <small>观测时间：{new Date(probe.observedAt).toLocaleString()} · 测试 5 分钟内有效</small>
                </div>
              )}
            </div>
            {selected && !selected.supported && (
              <p className="connection-error">尚未支持此产品。没有人工额度录入入口。</p>
            )}
          </div>
          <footer className="modal-footer">
            <button type="button" className="button button-quiet" onClick={onClose}>
              取消
            </button>
            <button
              type="submit"
              className="button button-primary"
              disabled={
                busy || !selected?.supported || (!value.id && !value.enabled) || (value.enabled && !probe)
              }
            >
              {busy ? "处理中…" : !value.id ? "添加账号并开始监控" : "保存连接"}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
