import { t } from "../lib/i18n";
import { APP_VERSION } from "../lib/version";

export function AboutPage() {
  return (
    <div className="about-page">
      <section className="about-card about-product" aria-label={t("about.product")}>
        <img src="/brand/stillenough.png" alt="" />
        <div>
          <h2>
            StillEnough <span className="about-version">v{APP_VERSION}</span>
          </h2>
          <p>{t("about.slogan")}</p>
          <p>{t("about.description")}</p>
        </div>
      </section>
      <section className="about-card">
        <h2>{t("about.privacy")}</h2>
        <p>{t("about.storage")}</p>
        <p>{t("about.queries")}</p>
      </section>
      <section className="about-card">
        <h2>{t("about.connections")}</h2>
        <p>{t("about.connectionHelp")}</p>
      </section>
    </div>
  );
}
