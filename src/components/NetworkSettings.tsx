import { providerName } from "../lib/providerPresentation";
import { t } from "../lib/i18n";
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
        if (active) setMessage(t("network.loadError"));
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
      setMessage(t("network.saved"));
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
        account.baseUrl === "cli://bailian/token-plan" ? t("network.cliSuccess") : t("network.testSuccess"),
      );
    } catch (error) {
      const code = String(error);
      setMessage(
        ["auth_required", "permission_denied", "rate_limited"].includes(code)
          ? t("network.testFailed", { reason: fetchStatusLabel(code) })
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
        <strong>{t("network.title")}</strong>
        <span>{t("network.scopeHelp")}</span>
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
            {t("network.testAccount")}
            <select
              value={accountId}
              onChange={(e) => {
                setAccountId(e.target.value);
                setMessage("");
              }}
            >
              <option value="">{t("network.chooseAccount")}</option>
              {accounts.map((a) => (
                <option key={a.id} value={a.id}>
                  {providerName(a.providerId, a.productName)} · {a.alias}
                </option>
              ))}
            </select>
          </label>
          {!accounts.length && <p className="settings-hint">{t("network.noAccounts")}</p>}
          <div className="settings-inline">
            <button
              type="button"
              className="button button-quiet"
              onClick={() => void test()}
              disabled={!accountId}
            >
              {t("network.test")}
            </button>
            <button type="button" className="button button-primary" onClick={() => void save()}>
              {t("network.save")}
            </button>
          </div>
        </fieldset>
        <span className="settings-hint">{t("network.providerHelp")}</span>
        {message && <p role="status">{fetchStatusLabel(message)}</p>}
      </div>
    </section>
  );
}
