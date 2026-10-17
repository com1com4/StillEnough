import { t } from "./i18n";
import { messages, type MessageKey } from "./locales";
import type { ProviderDescriptor } from "./types";

const fields = [
  "name",
  "capability",
  "status",
  "limitation",
  "setupInstructions",
  "authorizationLabel",
  "networkNote",
] as const;

export function providerName(providerId: string | undefined, fallback: string): string {
  const key = `provider.${providerId}.name` as MessageKey;
  if (Object.prototype.hasOwnProperty.call(messages, key)) return t(key);
  // Older history/alerts have a product name but no provider ID. Match exact
  // registered names only, keeping unknown products and user aliases intact.
  const known = Object.entries(messages).find(
    ([name, pair]) =>
      name.startsWith("provider.") && name.endsWith(".name") && pair.some((text) => text === fallback),
  );
  return known ? t(known[0] as MessageKey) : fallback;
}

export function providerOptionLabel(provider: ProviderDescriptor): string {
  const name = providerName(provider.id, provider.name);
  if (!provider.supported) return `${name} · ${localizeProvider(provider).status}`;
  const experimental = ["bailian_api", "bailian_token_plan"].includes(provider.id);
  return experimental ? `${name}${t("connection.experimental")}` : name;
}

/** The registry owns provider-specific copy; pages never branch on provider IDs. */
export function localizeProvider(provider: ProviderDescriptor): ProviderDescriptor {
  const localized = { ...provider };
  for (const field of fields) {
    const key = `provider.${provider.id}.${field}` as MessageKey;
    if (Object.prototype.hasOwnProperty.call(messages, key)) localized[field] = t(key);
  }
  return localized;
}
