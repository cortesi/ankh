import {
  AcceptOrgInvitePage,
  ForgotPasswordPage,
  LoginPage,
  Protected,
  ResetPasswordPage,
  SignupPage,
  useAuth,
  VerifyEmailPage,
} from "@ankh/auth-react";
import { Navigate, Route, Routes } from "react-router-dom";

import { Dashboard } from "./Dashboard";
import { CenteredPage, Layout } from "./Layout";

/** Send the index route to the dashboard when signed in, otherwise to login. */
function Home() {
  const { isLoading, user } = useAuth();
  if (isLoading) {
    return null;
  }
  return <Navigate replace to={user ? "/dashboard" : "/login"} />;
}

/** Demo route table wiring the shared @ankh/auth-react pages and a protected dashboard. */
export function App() {
  return (
    <Layout>
      <Routes>
        <Route element={<Home />} path="/" />
        <Route element={<CenteredPage>{<LoginPage />}</CenteredPage>} path="/login" />
        <Route element={<CenteredPage>{<SignupPage />}</CenteredPage>} path="/signup" />
        <Route
          element={<CenteredPage>{<ForgotPasswordPage />}</CenteredPage>}
          path="/forgot-password"
        />
        <Route
          element={<CenteredPage>{<ResetPasswordPage />}</CenteredPage>}
          path="/reset-password"
        />
        <Route element={<CenteredPage>{<VerifyEmailPage />}</CenteredPage>} path="/verify-email" />
        <Route
          element={<CenteredPage>{<AcceptOrgInvitePage />}</CenteredPage>}
          path="/accept-org-invite"
        />
        <Route
          element={
            <Protected>
              <Dashboard />
            </Protected>
          }
          path="/dashboard"
        />
        <Route element={<Navigate replace to="/" />} path="*" />
      </Routes>
    </Layout>
  );
}
