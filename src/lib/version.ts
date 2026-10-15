import { version } from "../../package.json";

// 构建时读取统一版本；不请求网络，不访问用户数据。
export const APP_VERSION = version;
