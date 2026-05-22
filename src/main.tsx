import React from "react";
import ReactDOM from "react-dom/client";
import App from "@/App";
import { Toaster } from "@/components/toaster";
import "@/index.css";

const rootElement = document.getElementById("root");
if (!rootElement) {
  throw new Error("missing #root element");
}

ReactDOM.createRoot(rootElement).render(
  <React.StrictMode>
    <App />
    <Toaster />
  </React.StrictMode>,
);
