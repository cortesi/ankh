import "@testing-library/jest-dom/vitest";

import type { AnkhApi } from "./api";
import type { DeviceSessionInfo, InviteInfo, MemberInfo, OrgInfo, UserInfo } from "@ankh/types";
import React from "react";
import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Link, MemoryRouter, Route, Routes } from "react-router-dom";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  AcceptOrgInvitePage,
  DeviceSessionsPanel,
  ForgotPasswordPage,
  LoginPage,
  OrgMembersPanel,
  ResetPasswordPage,
  SignupPage,
  VerificationBanner,
  VerifyEmailPage,
} from "./components";
import {
  AnkhApiProvider,
  AuthProvider,
  CurrentOrgProvider,
  Protected,
  useAuth,
  useCurrentOrg,
} from "./context";
import { validateEmail, validateOrgName, validatePassword } from "./validation";

const USER: UserInfo = {
  username: "alice",
  email: "alice@example.com",
  email_verified: true,
  waitlisted: false,
};

const ORG: OrgInfo = {
  id: "org-1",
  name: "test-org",
  display_name: "Test Org",
  role: "owner",
  is_personal: false,
};

const PERSONAL_ORG: OrgInfo = {
  id: "personal-org",
  name: "alice",
  display_name: "Alice",
  role: "owner",
  is_personal: true,
};

interface ApiOverrides {
  auth?: Partial<AnkhApi["auth"]>;
  orgs?: Partial<AnkhApi["orgs"]>;
  deviceSessions?: Partial<AnkhApi["deviceSessions"]>;
}

function createMockApi(overrides: ApiOverrides = {}): AnkhApi {
  const api: AnkhApi = {
    auth: {
      signup: vi.fn(async () => USER),
      login: vi.fn(async () => USER),
      logout: vi.fn(async () => undefined),
      getCurrentUser: vi.fn(async () => ({ user: null })),
      getWaitlistStatus: vi.fn(async () => false),
      verifyEmail: vi.fn(async () => undefined),
      resendVerification: vi.fn(async () => undefined),
      requestPasswordReset: vi.fn(async () => undefined),
      validateResetToken: vi.fn(async () => true),
      resetPassword: vi.fn(async () => undefined),
    },
    orgs: {
      listOrgs: vi.fn(async () => [ORG]),
      createOrg: vi.fn(async () => ORG),
      getOrg: vi.fn(async () => ORG),
      getMyMembership: vi.fn(async () => ({
        user_id: "user-1",
        username: USER.username,
        role: "owner",
      })),
      leaveOrg: vi.fn(async () => undefined),
      listOrgMembers: vi.fn(async () => []),
      removeOrgMember: vi.fn(async () => undefined),
      listOrgInvites: vi.fn(async () => []),
      inviteToOrg: vi.fn(async (_orgId, inviteEmail) => ({
        id: "invite-2",
        email: inviteEmail,
        created_at: "2026-01-01T00:00:00Z",
        expires_at: "2026-01-02T00:00:00Z",
      })),
      cancelInvite: vi.fn(async () => undefined),
      getOrgInviteDetails: vi.fn(async () => ({
        org_name: ORG.name,
        org_display_name: ORG.display_name,
        invite_email: "invitee@example.com",
      })),
      acceptOrgInvite: vi.fn(async () => ORG),
    },
    deviceSessions: {
      listDeviceSessions: vi.fn(async () => []),
      revokeDeviceSession: vi.fn(async () => undefined),
    },
  };

  return {
    auth: { ...api.auth, ...overrides.auth },
    orgs: { ...api.orgs, ...overrides.orgs },
    deviceSessions: { ...api.deviceSessions, ...overrides.deviceSessions },
  };
}

function renderWithAuth(children: React.ReactNode, api = createMockApi(), initialEntry = "/") {
  return render(
    <MemoryRouter initialEntries={[initialEntry]}>
      <AuthProvider api={api}>{children}</AuthProvider>
    </MemoryRouter>,
  );
}

function AuthProbe() {
  const { login, user } = useAuth();
  return (
    <button onClick={() => void login("alice@example.com", "password123")} type="button">
      {user ? user.email : "signed-out"}
    </button>
  );
}

function CurrentOrgProbe({ switchTo = PERSONAL_ORG.id }: { switchTo?: string }) {
  const { currentOrg, orgs, setCurrentOrgId } = useCurrentOrg();
  return (
    <button onClick={() => setCurrentOrgId(switchTo)} type="button">
      {currentOrg?.name ?? "loading"}:{orgs.length}
    </button>
  );
}

function UserProbe() {
  const { user } = useAuth();
  return <p>{user?.email ?? "signed-out"}</p>;
}

describe("@ankh/auth-react", () => {
  afterEach(() => {
    cleanup();
  });

  it("hydrates auth state and updates after login", async () => {
    const api = createMockApi();
    renderWithAuth(<AuthProbe />, api);

    expect(await screen.findByText("signed-out")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button"));
    expect(await screen.findByText(USER.email)).toBeInTheDocument();
    expect(api.auth.login).toHaveBeenCalledWith({
      email: "alice@example.com",
      password: "password123",
    });
  });

  it("validates shared auth and org inputs", () => {
    expect(validateEmail("bad")).toBe("Invalid email address");
    expect(validatePassword("short")).toBe("Password too short");
    expect(validateOrgName("api")).toBe("This name is reserved");
    expect(validateOrgName("valid-org")).toBeNull();
  });

  it("redirects protected routes when signed out", async () => {
    renderWithAuth(
      <Routes>
        <Route
          element={
            <Protected>
              <p>Secret</p>
            </Protected>
          }
          path="/secret"
        />
        <Route element={<p>Login route</p>} path="/login" />
      </Routes>,
      createMockApi(),
      "/secret",
    );

    expect(await screen.findByText("Login route")).toBeInTheDocument();
  });

  it("selects the route organization and allows switching organizations", async () => {
    const api = createMockApi({
      auth: {
        getCurrentUser: vi.fn(async () => ({ user: USER })),
      },
      orgs: {
        listOrgs: vi.fn(async () => [PERSONAL_ORG, ORG]),
      },
    });

    render(
      <MemoryRouter>
        <AuthProvider api={api}>
          <CurrentOrgProvider routeOrgId={ORG.id} storageKey={null}>
            <CurrentOrgProbe />
          </CurrentOrgProvider>
        </AuthProvider>
      </MemoryRouter>,
    );

    expect(await screen.findByRole("button", { name: "test-org:2" })).toBeInTheDocument();

    cleanup();
    render(
      <MemoryRouter>
        <AuthProvider api={api}>
          <CurrentOrgProvider storageKey={null}>
            <CurrentOrgProbe switchTo={ORG.id} />
          </CurrentOrgProvider>
        </AuthProvider>
      </MemoryRouter>,
    );

    expect(await screen.findByRole("button", { name: "alice:2" })).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button"));
    expect(await screen.findByRole("button", { name: "test-org:2" })).toBeInTheDocument();
  });

  it("submits login page through a custom frame slot", async () => {
    const api = createMockApi();
    renderWithAuth(
      <Routes>
        <Route
          element={
            <LoginPage
              dashboardPath="/app"
              frame={({ children, title }) => (
                <main>
                  <h1>{title}</h1>
                  {children}
                </main>
              )}
            />
          }
          path="/login"
        />
        <Route element={<p>App route</p>} path="/app" />
      </Routes>,
      api,
      "/login",
    );

    await screen.findByRole("heading", { name: "Sign in" });
    fireEvent.change(screen.getByLabelText("Email or Username"), {
      target: { value: "alice@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "password123" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Log in" }));

    expect(await screen.findByText("App route")).toBeInTheDocument();
  });

  it("clears cached auth state after resetting a password", async () => {
    const resetPassword = vi.fn(async () => undefined);
    const api = createMockApi({
      auth: {
        getCurrentUser: vi.fn(async () => ({ user: USER })),
        resetPassword,
      },
    });
    renderWithAuth(
      <Routes>
        <Route
          element={
            <>
              <UserProbe />
              <ResetPasswordPage />
            </>
          }
          path="/reset-password"
        />
        <Route element={<LoginPage dashboardPath="/app" />} path="/login" />
        <Route element={<p>App route</p>} path="/app" />
      </Routes>,
      api,
      "/reset-password?token=reset-token",
    );

    expect(await screen.findByText(USER.email)).toBeInTheDocument();
    fireEvent.change(await screen.findByLabelText("New password"), {
      target: { value: "password456" },
    });
    fireEvent.change(screen.getByLabelText("Confirm password"), {
      target: { value: "password456" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Save new password" }));

    expect(await screen.findByRole("heading", { name: "Sign in" })).toBeInTheDocument();
    expect(screen.queryByText("App route")).not.toBeInTheDocument();
    expect(resetPassword).toHaveBeenCalledWith("reset-token", "password456");
  });

  it("mounts an Ankh-only auth, org, and device harness", async () => {
    const member: MemberInfo = {
      user_id: "user-2",
      username: "bob",
      email: "bob@example.com",
      role: "member",
    };
    const session: DeviceSessionInfo = {
      id: "device-1",
      device_name: "Desktop",
      platform: "macos",
      status: "active",
      created_at: "2026-01-01T00:00:00Z",
      last_used_at: "2026-01-01T00:00:00Z",
      expires_at: "2026-01-02T00:00:00Z",
    };
    const api = createMockApi({
      orgs: {
        listOrgMembers: vi.fn(async () => [member]),
      },
      deviceSessions: {
        listDeviceSessions: vi.fn(async () => [session]),
      },
    });

    render(
      <MemoryRouter initialEntries={["/login"]}>
        <AuthProvider api={api}>
          <CurrentOrgProvider routeOrgId={ORG.id} storageKey={null}>
            <Routes>
              <Route
                element={
                  <LoginPage
                    dashboardPath={`/orgs/${ORG.id}/members`}
                    frame={({ children, title }) => (
                      <main>
                        <p>Ankh harness</p>
                        <h1>{title}</h1>
                        {children}
                      </main>
                    )}
                  />
                }
                path="/login"
              />
              <Route
                element={
                  <Protected>
                    <main>
                      <OrgMembersPanel canManage orgId={ORG.id} />
                      <Link to="/devices">Devices</Link>
                    </main>
                  </Protected>
                }
                path="/orgs/:id/members"
              />
              <Route
                element={
                  <Protected>
                    <DeviceSessionsPanel />
                  </Protected>
                }
                path="/devices"
              />
            </Routes>
          </CurrentOrgProvider>
        </AuthProvider>
      </MemoryRouter>,
    );

    expect(await screen.findByText("Ankh harness")).toBeInTheDocument();
    fireEvent.change(screen.getByLabelText("Email or Username"), {
      target: { value: "alice@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "password123" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Log in" }));

    expect(await screen.findByText("bob")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("link", { name: "Devices" }));
    expect(await screen.findByText("Desktop")).toBeInTheDocument();
  });

  it("accepts organization invites for authenticated users", async () => {
    const api = createMockApi({
      auth: {
        getCurrentUser: vi.fn(async () => ({ user: USER })),
      },
    });
    renderWithAuth(
      <Routes>
        <Route element={<AcceptOrgInvitePage />} path="/accept-org-invite" />
        <Route element={<p>Org route</p>} path="/orgs/:id" />
      </Routes>,
      api,
      "/accept-org-invite?token=invite-token",
    );

    expect(await screen.findByText("You were invited to join Test Org.")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Accept invite" }));
    expect(await screen.findByText("Org route")).toBeInTheDocument();
    expect(api.orgs.acceptOrgInvite).toHaveBeenCalledWith("invite-token");
  });

  it("deduplicates email verification requests", async () => {
    const verifyEmail = vi.fn(async () => undefined);
    const api = createMockApi({
      auth: {
        verifyEmail,
      },
    });

    renderWithAuth(
      <React.StrictMode>
        <Routes>
          <Route element={<VerifyEmailPage />} path="/verify-email" />
        </Routes>
      </React.StrictMode>,
      api,
      "/verify-email?token=verify-token",
    );

    expect(await screen.findByText("Your email has been verified.")).toBeInTheDocument();
    expect(verifyEmail).toHaveBeenCalledTimes(1);
  });

  it("resends verification emails and notifies callers", async () => {
    const notify = vi.fn();
    const resendVerification = vi.fn(async () => undefined);
    const api = createMockApi({
      auth: {
        resendVerification,
      },
    });

    render(
      <AnkhApiProvider api={api}>
        <VerificationBanner notify={notify} />
      </AnkhApiProvider>,
    );

    fireEvent.click(screen.getByRole("button", { name: "Resend verification email" }));

    await waitFor(() => expect(resendVerification).toHaveBeenCalledOnce());
    expect(notify).toHaveBeenCalledWith("Verification email sent.", "success");
  });

  it("loads, invites, removes, and cancels organization members", async () => {
    const member: MemberInfo = {
      user_id: "user-2",
      username: "bob",
      email: "bob@example.com",
      role: "member",
    };
    const invite: InviteInfo = {
      id: "invite-1",
      email: "pending@example.com",
      created_at: "2026-01-01T00:00:00Z",
      expires_at: "2026-01-02T00:00:00Z",
    };
    const api = createMockApi({
      orgs: {
        listOrgMembers: vi.fn(async () => [member]),
        listOrgInvites: vi.fn(async () => [invite]),
      },
    });

    render(
      <AnkhApiProvider api={api}>
        <OrgMembersPanel canManage orgId={ORG.id} />
      </AnkhApiProvider>,
    );

    expect(await screen.findByText("bob")).toBeInTheDocument();
    fireEvent.change(screen.getByPlaceholderText("Email address"), {
      target: { value: "new@example.com" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Invite" }));
    expect(await screen.findByText("new@example.com")).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "Remove bob" }));
    await waitFor(() => expect(screen.queryByText("bob")).not.toBeInTheDocument());
    fireEvent.click(screen.getByRole("button", { name: "Cancel invite for pending@example.com" }));
    await waitFor(() => expect(screen.queryByText("pending@example.com")).not.toBeInTheDocument());
  });

  it("lists and revokes device sessions", async () => {
    const session: DeviceSessionInfo = {
      id: "device-1",
      device_name: "Desktop",
      platform: "macos",
      status: "active",
      created_at: "2026-01-01T00:00:00Z",
      last_used_at: "2026-01-01T00:00:00Z",
      expires_at: "2026-01-02T00:00:00Z",
    };
    const api = createMockApi({
      deviceSessions: {
        listDeviceSessions: vi.fn(async () => [session]),
      },
    });

    render(
      <AnkhApiProvider api={api}>
        <DeviceSessionsPanel />
      </AnkhApiProvider>,
    );

    expect(await screen.findByText("Desktop")).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "Revoke" }));
    await waitFor(() => expect(screen.queryByText("Desktop")).not.toBeInTheDocument());
    expect(api.deviceSessions.revokeDeviceSession).toHaveBeenCalledWith("device-1");
  });

  it("submits the signup page and navigates to the dashboard", async () => {
    const signup = vi.fn(async () => USER);
    const api = createMockApi({ auth: { signup } });
    renderWithAuth(
      <Routes>
        <Route element={<SignupPage dashboardPath="/app" />} path="/signup" />
        <Route element={<p>App route</p>} path="/app" />
      </Routes>,
      api,
      "/signup",
    );

    await screen.findByRole("heading", { name: "Create account" });
    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "carol@example.com" },
    });
    fireEvent.change(screen.getByLabelText("Username"), {
      target: { value: "carol" },
    });
    fireEvent.change(screen.getByLabelText("Password"), {
      target: { value: "password123" },
    });
    fireEvent.change(screen.getByLabelText("Confirm password"), {
      target: { value: "password123" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Sign up" }));

    expect(await screen.findByText("App route")).toBeInTheDocument();
    expect(signup).toHaveBeenCalled();
  });

  it("requests a password reset and confirms submission", async () => {
    const requestPasswordReset = vi.fn(async () => undefined);
    const api = createMockApi({ auth: { requestPasswordReset } });
    renderWithAuth(
      <Routes>
        <Route element={<ForgotPasswordPage />} path="/forgot" />
      </Routes>,
      api,
      "/forgot",
    );

    await screen.findByRole("heading", { name: "Forgot password" });
    fireEvent.change(screen.getByLabelText("Email"), {
      target: { value: "alice@example.com" },
    });
    fireEvent.click(screen.getByRole("button", { name: "Send reset email" }));

    expect(
      await screen.findByText("If an account exists, a reset link is on its way."),
    ).toBeInTheDocument();
    expect(requestPasswordReset).toHaveBeenCalledWith("alice@example.com");
  });
});
