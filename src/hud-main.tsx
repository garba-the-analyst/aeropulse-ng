import React from "react";
import { createRoot } from "react-dom/client";
import HudApp from "./HudApp";
import "./styles.css";

const el = document.getElementById("hud-root") as HTMLElement;
createRoot(el).render(
  <React.StrictMode>
    <HudApp />
  </React.StrictMode>,
);
