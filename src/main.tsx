import "@fontsource-variable/inter";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App";
import { TraceMini } from "./components/TraceMini";
import "./styles.css";

// 安装监控的浮动小窗和主窗口用同一个页面，地址带 ?mini=trace
const mini = new URLSearchParams(location.search).get("mini") === "trace";
if (mini) {
  // 小窗自己画圆角，窗口背景透明
  document.documentElement.style.background = "transparent";
  document.body.style.background = "transparent";
}

createRoot(document.getElementById("root")!).render(<StrictMode>{mini ? <TraceMini /> : <App />}</StrictMode>);
