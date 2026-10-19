import { useState } from "react";
import { MagnifyingGlassIcon, SquaresFourIcon } from "@phosphor-icons/react";
import { ProviderIcon } from "./ProviderIcon";
import { t } from "../lib/i18n";
import { filterPickerProviders, pickerEntry } from "../lib/providerPicker";
import { providerOptionLabel } from "../lib/providerPresentation";
import type { ProviderDescriptor } from "../lib/types";

export function ProductPicker({
  providers,
  selectedId,
  onSelect,
}: {
  providers: ProviderDescriptor[];
  selectedId: string;
  onSelect: (provider: ProviderDescriptor) => void;
}) {
  const [query, setQuery] = useState("");
  const [platform, setPlatform] = useState(() => {
    const selected = providers.find((p) => p.id === selectedId);
    return selected ? pickerEntry(selected).platform : "";
  });
  const matches = filterPickerProviders(providers, query).map(pickerEntry);
  const platforms = [...new Map(matches.map((p) => [p.platform, p])).values()];
  const activePlatform = platforms.some((p) => p.platform === platform) ? platform : "";
  const visible = matches.filter((p) => !activePlatform || p.platform === activePlatform);
  const products = [...new Map(visible.map((p) => [`${p.platform}/${p.product}`, p])).values()];
  const selected = providers.find((p) => p.id === selectedId);
  const selectedEntry = selected ? pickerEntry(selected) : null;
  return (
    <div className="product-picker">
      <label className="picker-search">
        <MagnifyingGlassIcon aria-hidden />
        <input
          type="search"
          value={query}
          onChange={(e) => setQuery(e.target.value)}
          placeholder={t("picker.search")}
          aria-label={t("picker.search")}
        />
      </label>
      <div className="picker-browser">
        <nav className="picker-platforms" aria-label={t("picker.platforms")}>
          <button
            type="button"
            className={!activePlatform ? "selected" : ""}
            aria-pressed={!activePlatform}
            onClick={() => setPlatform("")}
          >
            <SquaresFourIcon aria-hidden />
            {t("picker.all")}
          </button>
          {platforms.map((p) => (
            <button
              type="button"
              key={p.platform}
              className={activePlatform === p.platform ? "selected" : ""}
              aria-pressed={activePlatform === p.platform}
              onClick={() => setPlatform(p.platform)}
            >
              <ProviderIcon product={p.provider.name} />
              {p.platformLabel}
            </button>
          ))}
        </nav>
        <div className="picker-products">
          <h3>{platforms.find((p) => p.platform === activePlatform)?.platformLabel ?? t("picker.all")}</h3>
          <p className="settings-hint">{t("connection.chooseProduct")}</p>
          <div className="picker-product-list">
            {products.map((p) => {
              const variants = visible.filter((v) => v.platform === p.platform && v.product === p.product);
              const chosen = variants.find((v) => v.provider.id === selectedId);
              return (
                <div key={`${p.platform}/${p.product}`} className="picker-product-group">
                  <button
                    type="button"
                    className={`picker-product ${chosen ? "selected" : ""}`}
                    aria-pressed={!!chosen}
                    onClick={() => onSelect((chosen ?? variants[0]).provider)}
                  >
                    <span className="picker-radio" aria-hidden />
                    <span>
                      <strong>
                        {!activePlatform && `${p.platformLabel} · `}
                        {p.productLabel}
                      </strong>
                      <small>{p.provider.capability}</small>
                    </span>
                  </button>
                  {chosen && (
                    <div className="picker-product-detail">
                      {variants.some((v) => v.region !== "default") && (
                        <>
                          <span className="picker-region-label">{t("picker.region")}</span>
                          <div className="picker-regions" role="group" aria-label={t("picker.region")}>
                            {variants.map((v) => (
                              <button
                                type="button"
                                key={v.provider.id}
                                className={selectedId === v.provider.id ? "selected" : ""}
                                aria-pressed={selectedId === v.provider.id}
                                onClick={() => onSelect(v.provider)}
                              >
                                {v.regionLabel}
                              </button>
                            ))}
                          </div>
                          <p className="settings-hint">{t("picker.regionHint")}</p>
                        </>
                      )}
                      <p className="picker-status">{providerOptionLabel(chosen.provider)}</p>
                      <p className="settings-hint">{chosen.provider.limitation}</p>
                    </div>
                  )}
                </div>
              );
            })}
            {!products.length && (
              <p role="status">{providers.length ? t("picker.empty") : t("picker.loading")}</p>
            )}
          </div>
          <p className="picker-note">{t("picker.hint")}</p>
        </div>
      </div>
      <div className="picker-summary" aria-live="polite">
        {t("picker.selected")}：{" "}
        <strong>
          {selectedEntry
            ? `${selectedEntry.platformLabel} / ${selectedEntry.productLabel} / ${selectedEntry.regionLabel}`
            : t("connection.chooseProduct")}
        </strong>
      </div>
    </div>
  );
}
