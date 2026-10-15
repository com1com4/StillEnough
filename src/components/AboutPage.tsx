import { APP_VERSION } from "../lib/version";

export function AboutPage() {
  return (
    <div className="about-page">
      <section className="about-card about-product" aria-label="产品信息">
        <img src="/brand/stillenough.png" alt="" />
        <div>
          <h2>
            StillEnough <span className="about-version">v{APP_VERSION}</span>
          </h2>
          <p>看一眼，安心继续。</p>
          <p>集中查看多个 AI 平台的额度、重置时间与 API 账号余额。</p>
        </div>
      </section>
      <section className="about-card">
        <h2>数据与隐私</h2>
        <p>
          账号配置、设置和观测历史保存在本机。API Key 保存在 Windows 系统凭据库；本地 CLI
          登录来源仅在你明确授权后读取。
        </p>
        <p>
          查询时连接相应平台；额度与余额以平台实际返回为准。订阅额度与 API 钱包余额分别展示，未知值不推算。
        </p>
      </section>
      <section className="about-card">
        <h2>接入说明</h2>
        <p>
          各平台的连接方式、支持范围与验证状态见“添加账号”。实验性接口可能变化，查询失败时请查看账号状态和最近成功更新时间。
        </p>
      </section>
    </div>
  );
}
