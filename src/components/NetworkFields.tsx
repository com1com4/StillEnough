import { t } from "../lib/i18n";
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
        {t("network.mode")}
        <select
          value={value.mode}
          onChange={(e) => onChange({ mode: e.target.value as NetworkConfig["mode"], proxyUrl: "" })}
        >
          {!global && <option value="inherit">{t("network.inherit")}</option>}
          <option value="system">{t("common.system")}</option>
          <option value="direct">{t("network.direct")}</option>
          <option value="custom">{t("network.custom")}</option>
        </select>
      </label>
      {value.mode === "custom" && (
        <label>
          {t("network.proxyUrl")}
          <input
            value={value.proxyUrl}
            maxLength={512}
            placeholder="http://127.0.0.1:7890"
            autoComplete="off"
            onChange={(e) => onChange({ ...value, proxyUrl: e.target.value.trim() })}
          />
          <span className="settings-hint">{t("network.protocolHelp")}</span>
        </label>
      )}
      {value.mode === "system" && <p className="settings-hint">{t("network.systemHelp")}</p>}
    </>
  );
}
