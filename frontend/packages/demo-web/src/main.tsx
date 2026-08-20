import { AuthProvider, CurrentOrgProvider } from "@ankh/auth-react";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { BrowserRouter } from "react-router-dom";

import { App } from "./App";

import "@ankh/ui/ankh.css";
import "./demo.css";

const container = document.getElementById("root");
if (!container) {
  throw new Error("missing #root element");
}

createRoot(container).render(
  <StrictMode>
    <BrowserRouter>
      <AuthProvider>
        <CurrentOrgProvider>
          <App />
        </CurrentOrgProvider>
      </AuthProvider>
    </BrowserRouter>
  </StrictMode>,
);
