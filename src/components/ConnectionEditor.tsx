import { t, currentLanguage } from "../lib/i18n";
import { NetworkFields } from "./NetworkFields";
import { useEffect, useRef, useState } from "react";
import type { AccountInput, ProbeResult, ProviderDescriptor } from "../lib/types";
import { api } from "../lib/ipc";
import { balanceText, balanceLabel, fetchStatusLabel, metricLabel } from "../lib/presentation";
import { XIcon } from "@phosphor-icons/react";
import { localizeProvider, providerOptionLabel } from "../lib/providerPresentation";

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
  const localizedProviders = providers.map(localizeProvider);
  const selected = localizedProviders.find((p) => p.id === value.providerId);
  // Temporarily hide app-server from new connections; existing accounts remain manageable.
  const selectableProviders = localizedProviders.filter((p) => p.id !== "codex" || value.id !== null);
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
    if (!selected?.supported) throw new Error(t("connection.unsupported"));
    if (!value.alias.trim()) throw new Error(t("connection.aliasRequired"));
    if (isCli && requireCliAuthorization && !value.cliAuthorized)
      throw new Error(t("connection.authorizationRequired"));
    if (!session.current) {
      const id = await api.beginSetup(value);
      if (!mounted.current) {
        void api.cancelSetup(id);
        throw new Error(t("connection.closed"));
      }
      session.current = id;
    }
    if (!isCli && secret.current?.value) {
      if (selected.authMethod === "access_key_pair" && !accessKeyId.current?.value.trim())
        throw new Error(t("connection.accessIdRequired"));
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
            <h2 id="connection-heading">{value.id ? t("connection.manage") : t("account.add")}</h2>
          </div>
          <button className="icon-button" onClick={onClose} aria-label={t("common.close")}>
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
            <p className="connection-flow">{t("connection.flow")}</p>
            <fieldset disabled={busy} className="connection-fields">
              <label>
                {t("connection.product")}
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
                  <option value="">{t("connection.chooseProduct")}</option>
                  {selectableProviders.map((p) => (
                    <option key={p.id} value={p.id}>
                      {providerOptionLabel(p)}
                    </option>
                  ))}
                </select>
              </label>
              {selected && (
                <details className="connection-capability connection-notes">
                  <summary>
                    {selected.capability}
                    <span>{t("connection.details")}</span>
                  </summary>
                  <p>{selected.limitation}</p>
                </details>
              )}
              {selected?.supported && (
                <>
                  <label>
                    {t("account.alias")}
                    <input
                      required
                      maxLength={80}
                      value={value.alias}
                      onChange={(e) => update({ alias: e.target.value })}
                      placeholder={t("connection.aliasPlaceholder")}
                    />
                  </label>
                  <details>
                    <summary>{t("network.mode")}</summary>
                    <NetworkFields
                      value={value.network ?? { mode: "inherit", proxyUrl: "" }}
                      onChange={(network) => update({ network })}
                    />
                    {isCli && <p className="settings-hint">{selected.networkNote}</p>}
                  </details>
                  {isCli ? (
                    <div className="connection-capability">
                      <strong>{t("connection.localLogin")}</strong>
                      <pre>{selected.setupInstructions}</pre>
                      <p>{t("connection.localHelp")}</p>
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
                        {t("connection.url")}
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
                            placeholder={t("connection.accessIdPlaceholder")}
                          />
                        </label>
                      )}
                      <label>
                        {value.id ? t("connection.replaceKey", { label: credentialLabel }) : credentialLabel}
                        <input
                          ref={secret}
                          type="password"
                          autoComplete="off"
                          spellCheck={false}
                          onChange={invalidate}
                          placeholder={value.id ? t("connection.keyStored") : t("connection.keyPlaceholder")}
                        />
                      </label>
                      <p className="privacy-note">{t("connection.keyPrivacy")}</p>
                    </>
                  )}
                  <label className="connection-enable">
                    <input
                      type="checkbox"
                      checked={value.enabled}
                      onChange={(e) => update({ enabled: e.target.checked })}
                    />
                    {isCli
                      ? t("connection.enableCli")
                      : value.id
                        ? t("connection.enableApi")
                        : t("connection.consent")}
                  </label>
                  <p className="privacy-note">
                    {isCli ? t("connection.cliTestHelp") : t("connection.apiTestHelp")}
                  </p>
                  <button
                    type="button"
                    className="button button-quiet"
                    onClick={() => void test()}
                    disabled={busy || (isCli && !value.cliAuthorized)}
                  >
                    {busy ? t("common.busy") : t("connection.test")}
                  </button>
                </>
              )}
            </fieldset>
            <div ref={feedback} aria-live="polite" aria-busy={busy}>
              {busy && (
                <p className="privacy-note" role="status">
                  {t("common.processing")}
                </p>
              )}
              {error && (
                <p className="connection-error" role="alert">
                  {fetchStatusLabel(error)}
                </p>
              )}
              {probe && (
                <div className="connection-result" role="status">
                  <strong>{isCli ? t("connection.cliSuccess") : t("connection.apiSuccess")}</strong>
                  {probe.metrics.map((m) => (
                    <p key={m.metricId}>
                      {m.kind === "balance" ? balanceLabel(m) : metricLabel(m.label)}：
                      {(m.kind === "balance" ? balanceText(m) : m.remaining) ??
                        (m.remainingPercent != null
                          ? t("metric.remainingPercent", {
                              percent: Math.round(m.remainingPercent * 10) / 10,
                            })
                          : t("metric.unknown"))}
                    </p>
                  ))}
                  <small>
                    {t("observation.time")}
                    {new Date(probe.observedAt).toLocaleString(currentLanguage())}{" "}
                    {t("connection.testValidity")}
                  </small>
                </div>
              )}
            </div>
            {selected && !selected.supported && (
              <p className="connection-error">{t("connection.noManual")}</p>
            )}
          </div>
          <footer className="modal-footer">
            <button type="button" className="button button-quiet" onClick={onClose}>
              {t("common.cancel")}
            </button>
            <button
              type="submit"
              className="button button-primary"
              disabled={
                busy || !selected?.supported || (!value.id && !value.enabled) || (value.enabled && !probe)
              }
            >
              {busy ? t("common.busy") : !value.id ? t("connection.addMonitor") : t("connection.save")}
            </button>
          </footer>
        </form>
      </section>
    </div>
  );
}
