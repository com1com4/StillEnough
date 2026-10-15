import { CpuIcon } from "@phosphor-icons/react";
import { productBrand } from "../lib/brands";

export function ProviderIcon({ product }: { product: string }) {
  const brand = productBrand(product);
  return brand ? (
    <span
      className="provider-icon"
      role="img"
      aria-label={brand.name}
      style={{ backgroundColor: brand.color, maskImage: `url(/providers/${brand.icon}.svg)` }}
    />
  ) : (
    <CpuIcon className="provider-icon" aria-hidden />
  );
}
