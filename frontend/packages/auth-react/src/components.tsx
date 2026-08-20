import type { DeviceSessionInfo, InviteInfo, MemberInfo, OrgInfo } from "@ankh/types";
import { useEffect, useState, type FormEvent, type ReactNode } from "react";
import { Link, Navigate, useLocation, useNavigate, useSearchParams } from "react-router-dom";

import {
  Alert,
  Button,
  Card,
  CardContent,
  CardDescription,
  CardHeader,
  CardTitle,
  Field,
  FieldDescription,
  FieldGroup,
  FieldLabel,
  Input,
  Spinner,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableHeader,
  TableRow,
} from "@ankh/ui";

import { asMessage } from "./api";
import { useAnkhApi, useAuth, useCurrentOrg } from "./context";
import {
  normalizeName,
  validateEmail,
  validateLoginIdentifier,
  validateOrgName,
  validatePassword,
  validateUsername,
} from "./validation";

export interface FrameProps {
  title: string;
  description?: ReactNode;
  children: ReactNode;
}

export type FrameComponent = (props: FrameProps) => ReactNode;
export type Notify = (message: string, variant?: "success" | "error") => void;

export interface SharedPageProps {
  frame?: FrameComponent;
  dashboardPath?: string;
  loginPath?: string;
  signupPath?: string;
  forgotPasswordPath?: string;
  resolveRedirect?: (state: unknown) => string;
}

export interface PanelProps {
  className?: string;
  emptyMessage?: string;
  formatDate?: (value: string) => string;
  notify?: Notify;
}

export interface OrgMembersPanelProps extends PanelProps {
  canManage?: boolean;
  orgId: string;
}

export interface DeviceSessionsPanelProps extends PanelProps {}

export interface NewOrgFormProps {
  className?: string;
  onCreated?: (org: OrgInfo) => void | Promise<void>;
  notify?: Notify;
}

export interface VerificationBannerProps {
  className?: string;
  notify?: Notify;
}

const verificationRequests = new Map<string, Promise<"success" | "error">>();

export function DefaultFrame({ children, description, title }: FrameProps) {
  return (
    <Card className="ankh-auth-card">
      <CardHeader>
        <CardTitle>{title}</CardTitle>
        {description ? <CardDescription>{description}</CardDescription> : null}
      </CardHeader>
      <CardContent>{children}</CardContent>
    </Card>
  );
}

export function LoginPage({
  dashboardPath = "/dashboard",
  forgotPasswordPath = "/forgot-password",
  frame = DefaultFrame,
  resolveRedirect = defaultResolveRedirect(dashboardPath),
  signupPath = "/signup",
}: SharedPageProps) {
  const { login, user } = useAuth();
  const location = useLocation();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  if (user) {
    return <Navigate replace to={resolveRedirect(location.state)} />;
  }

  return frame({
    title: "Sign in",
    description: "Use your email or username to continue.",
    children: (
      <>
        <Notice error={error} />
        <form
          className="ankh-form"
          noValidate
          onSubmit={async (event) => {
            event.preventDefault();
            const identifierError = validateLoginIdentifier(email);
            if (identifierError) {
              setError(identifierError);
              return;
            }
            if (!password.trim()) {
              setError("Password is required");
              return;
            }
            setError(null);
            try {
              await login(email, password);
              navigate(searchParams.get("redirect") ?? resolveRedirect(location.state));
            } catch (caught) {
              setError(asMessage(caught));
            }
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="login-email">Email or Username</FieldLabel>
              <Input
                autoComplete="username"
                id="login-email"
                onChange={(event) => setEmail(event.target.value)}
                type="text"
                value={email}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="login-password">Password</FieldLabel>
              <Input
                autoComplete="current-password"
                id="login-password"
                onChange={(event) => setPassword(event.target.value)}
                type="password"
                value={password}
              />
            </Field>
          </FieldGroup>
          <Button id="login-submit" type="submit">
            Log in
          </Button>
          <nav className="ankh-auth-links">
            <Link to={signupPath}>Need an account?</Link>
            <Link to={forgotPasswordPath}>Forgot password?</Link>
          </nav>
        </form>
      </>
    ),
  });
}

export function SignupPage({
  dashboardPath = "/dashboard",
  frame = DefaultFrame,
  loginPath = "/login",
}: SharedPageProps) {
  const api = useAnkhApi();
  const { signup, user } = useAuth();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const [waitlistEnabled, setWaitlistEnabled] = useState(false);
  const [username, setUsername] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    void api.auth
      .getWaitlistStatus()
      .then(setWaitlistEnabled)
      .catch(() => setWaitlistEnabled(false));
  }, [api]);

  if (user) {
    return <Navigate replace to={dashboardPath} />;
  }

  const inviteToken = searchParams.get("invite");
  const orgInviteToken = searchParams.get("org_invite");

  return frame({
    title: "Create account",
    children: (
      <>
        {waitlistEnabled && !inviteToken ? (
          <Alert>
            Signups are waitlisted right now — create an account to join the waitlist and we'll
            email you when your spot is ready.
          </Alert>
        ) : null}
        <Notice error={error} />
        <form
          className="ankh-form"
          noValidate
          onSubmit={async (event) => {
            event.preventDefault();
            const emailError = validateEmail(email);
            const usernameError = validateUsername(username);
            const passwordError = validatePassword(password);
            if (emailError || usernameError || passwordError) {
              setError(emailError ?? usernameError ?? passwordError);
              return;
            }
            if (password !== confirmPassword) {
              setError("Passwords do not match");
              return;
            }
            setError(null);
            try {
              await signup(
                normalizeName(username),
                email.trim().toLowerCase(),
                password,
                inviteToken,
                orgInviteToken,
              );
              navigate(dashboardPath);
            } catch (caught) {
              setError(asMessage(caught));
            }
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="signup-email">Email</FieldLabel>
              <Input
                autoComplete="email"
                id="signup-email"
                onChange={(event) => setEmail(event.target.value)}
                type="email"
                value={email}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="signup-username">Username</FieldLabel>
              <Input
                autoComplete="username"
                id="signup-username"
                onChange={(event) => setUsername(event.target.value)}
                type="text"
                value={username}
              />
            </Field>
            <Field>
              <FieldLabel htmlFor="signup-password">Password</FieldLabel>
              <Input
                autoComplete="new-password"
                id="signup-password"
                onChange={(event) => setPassword(event.target.value)}
                type="password"
                value={password}
              />
              <FieldDescription>Use at least 8 characters.</FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="signup-confirm-password">Confirm password</FieldLabel>
              <Input
                autoComplete="new-password"
                id="signup-confirm-password"
                onChange={(event) => setConfirmPassword(event.target.value)}
                type="password"
                value={confirmPassword}
              />
            </Field>
          </FieldGroup>
          <Button id="signup-submit" type="submit">
            Sign up
          </Button>
          <nav className="ankh-auth-links">
            <Link to={loginPath}>Already have an account?</Link>
          </nav>
        </form>
      </>
    ),
  });
}

export function ForgotPasswordPage({
  frame = DefaultFrame,
  loginPath = "/login",
}: SharedPageProps) {
  const api = useAnkhApi();
  const [email, setEmail] = useState("");
  const [error, setError] = useState<string | null>(null);
  const [submitted, setSubmitted] = useState(false);

  return frame({
    title: "Forgot password",
    description: "Enter your email and we will send a reset link if an account exists.",
    children: (
      <>
        <Notice
          error={error}
          success={submitted ? "If an account exists, a reset link is on its way." : null}
        />
        {!submitted ? (
          <form
            className="ankh-form"
            noValidate
            onSubmit={async (event) => {
              event.preventDefault();
              const emailError = validateEmail(email);
              if (emailError) {
                setError(emailError);
                return;
              }
              setError(null);
              try {
                await api.auth.requestPasswordReset(email.trim().toLowerCase());
                setSubmitted(true);
              } catch (caught) {
                setError(asMessage(caught));
              }
            }}
          >
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="forgot-email">Email</FieldLabel>
                <Input
                  autoComplete="email"
                  id="forgot-email"
                  onChange={(event) => setEmail(event.target.value)}
                  type="email"
                  value={email}
                />
              </Field>
            </FieldGroup>
            <Button id="forgot-submit" type="submit">
              Send reset email
            </Button>
          </form>
        ) : null}
        <Link to={loginPath}>Back to sign in</Link>
      </>
    ),
  });
}

export function ResetPasswordPage({ frame = DefaultFrame, loginPath = "/login" }: SharedPageProps) {
  const api = useAnkhApi();
  const { clearAuthState } = useAuth();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const token = searchParams.get("token") ?? "";
  const [tokenState, setTokenState] = useState<"checking" | "valid" | "invalid">("checking");
  const [password, setPassword] = useState("");
  const [confirmPassword, setConfirmPassword] = useState("");
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) {
      setTokenState("invalid");
      return;
    }
    void api.auth
      .validateResetToken(token)
      .then((valid) => setTokenState(valid ? "valid" : "invalid"))
      .catch(() => setTokenState("invalid"));
  }, [api, token]);

  return frame({
    title: "Reset password",
    children:
      tokenState === "checking" ? (
        <Alert>
          <Spinner /> Checking reset link...
        </Alert>
      ) : tokenState === "invalid" ? (
        <>
          <Alert variant="error">This reset link is invalid or expired.</Alert>
          <Link to={loginPath}>Back to sign in</Link>
        </>
      ) : (
        <>
          <Notice error={error} />
          <form
            className="ankh-form"
            onSubmit={async (event) => {
              event.preventDefault();
              const passwordError = validatePassword(password);
              if (passwordError) {
                setError(passwordError);
                return;
              }
              if (password !== confirmPassword) {
                setError("Passwords do not match");
                return;
              }
              setError(null);
              try {
                await api.auth.resetPassword(token, password);
                clearAuthState();
                navigate(loginPath);
              } catch (caught) {
                setError(asMessage(caught));
              }
            }}
          >
            <FieldGroup>
              <Field>
                <FieldLabel htmlFor="reset-password">New password</FieldLabel>
                <Input
                  autoComplete="new-password"
                  id="reset-password"
                  onChange={(event) => setPassword(event.target.value)}
                  type="password"
                  value={password}
                />
              </Field>
              <Field>
                <FieldLabel htmlFor="reset-confirm-password">Confirm password</FieldLabel>
                <Input
                  autoComplete="new-password"
                  id="reset-confirm-password"
                  onChange={(event) => setConfirmPassword(event.target.value)}
                  type="password"
                  value={confirmPassword}
                />
              </Field>
            </FieldGroup>
            <Button id="reset-submit" type="submit">
              Save new password
            </Button>
          </form>
        </>
      ),
  });
}

export function VerifyEmailPage({
  dashboardPath = "/dashboard",
  frame = DefaultFrame,
}: SharedPageProps) {
  const api = useAnkhApi();
  const [searchParams] = useSearchParams();
  const [status, setStatus] = useState<"idle" | "success" | "error">("idle");
  const token = searchParams.get("token");

  useEffect(() => {
    if (!token) {
      setStatus("error");
      return;
    }
    let request = verificationRequests.get(token);
    if (!request) {
      request = api.auth
        .verifyEmail(token)
        .then(() => "success" as const)
        .catch(() => "error" as const);
      verificationRequests.set(token, request);
    }
    void request.then(setStatus);
  }, [api, token]);

  return frame({
    title: "Verify email",
    children: (
      <>
        {status === "success" ? (
          <Alert variant="success">Your email has been verified.</Alert>
        ) : status === "error" ? (
          <Alert variant="error">This verification link is invalid or expired.</Alert>
        ) : (
          <Alert>
            <Spinner /> Checking your verification link...
          </Alert>
        )}
        <Link to={dashboardPath}>Return to dashboard</Link>
      </>
    ),
  });
}

export function VerificationBanner({ className, notify }: VerificationBannerProps) {
  const api = useAnkhApi();
  const [error, setError] = useState<string | null>(null);

  return (
    <Alert className={className}>
      <span>Your email is not verified yet.</span>
      <Button
        id="resend-verification-button"
        onClick={async () => {
          try {
            await api.auth.resendVerification();
            setError(null);
            notify?.("Verification email sent.", "success");
          } catch (caught) {
            const message = asMessage(caught);
            setError(message);
            notify?.(message, "error");
          }
        }}
        size="sm"
        variant="ghost"
      >
        Resend verification email
      </Button>
      {error ? <span>{error}</span> : null}
    </Alert>
  );
}

export function AcceptOrgInvitePage({
  frame = DefaultFrame,
  loginPath = "/login",
  signupPath = "/signup",
}: SharedPageProps) {
  const api = useAnkhApi();
  const { user } = useAuth();
  const navigate = useNavigate();
  const [searchParams] = useSearchParams();
  const token = searchParams.get("token") ?? "";
  const [details, setDetails] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (!token) {
      return;
    }
    void api.orgs
      .getOrgInviteDetails(token)
      .then((invite) => setDetails(invite.org_display_name ?? invite.org_name))
      .catch((caught) => setError(asMessage(caught)));
  }, [api, token]);

  return frame({
    title: "Organization invite",
    children: (
      <>
        {details ? <p>You were invited to join {details}.</p> : null}
        <Notice error={error} />
        {!user ? (
          <nav className="ankh-auth-links">
            <Link
              to={`${loginPath}?redirect=${encodeURIComponent(`/accept-org-invite?token=${token}`)}`}
            >
              Log in
            </Link>
            <Link to={`${signupPath}?org_invite=${encodeURIComponent(token)}`}>Create account</Link>
          </nav>
        ) : (
          <Button
            id="accept-org-invite"
            onClick={async () => {
              const org = await api.orgs.acceptOrgInvite(token);
              navigate(`/orgs/${org.id}`);
            }}
          >
            Accept invite
          </Button>
        )}
      </>
    ),
  });
}

export function NewOrgForm({ className, notify, onCreated }: NewOrgFormProps) {
  const api = useAnkhApi();
  const { reload, setCurrentOrgId } = useCurrentOrg();
  const [name, setName] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [error, setError] = useState<string | null>(null);

  return (
    <Card className={className}>
      <CardHeader>
        <CardTitle>Create organization</CardTitle>
      </CardHeader>
      <CardContent>
        <Notice error={error} />
        <form
          className="ankh-form"
          onSubmit={async (event) => {
            event.preventDefault();
            const normalizedName = normalizeName(name);
            const nameError = validateOrgName(normalizedName);
            if (nameError) {
              setError(nameError);
              return;
            }
            try {
              const org = await api.orgs.createOrg({
                name: normalizedName,
                display_name: displayName.trim() || null,
              });
              await reload();
              setCurrentOrgId(org.id);
              await onCreated?.(org);
              notify?.(`Created ${org.display_name ?? org.name}.`, "success");
            } catch (caught) {
              setError(asMessage(caught));
            }
          }}
        >
          <FieldGroup>
            <Field>
              <FieldLabel htmlFor="org-name">Name</FieldLabel>
              <Input id="org-name" onChange={(event) => setName(event.target.value)} value={name} />
              <FieldDescription>
                Organization names use lowercase letters, numbers, and hyphens.
              </FieldDescription>
            </Field>
            <Field>
              <FieldLabel htmlFor="org-display-name">Display name</FieldLabel>
              <Input
                id="org-display-name"
                onChange={(event) => setDisplayName(event.target.value)}
                value={displayName}
              />
            </Field>
          </FieldGroup>
          <Button id="org-submit" type="submit">
            Create organization
          </Button>
        </form>
      </CardContent>
    </Card>
  );
}

export function OrgMembersPanel({
  canManage = false,
  className,
  emptyMessage = "No members yet.",
  formatDate = defaultFormatDate,
  notify,
  orgId,
}: OrgMembersPanelProps) {
  const api = useAnkhApi();
  const [members, setMembers] = useState<MemberInfo[]>([]);
  const [invites, setInvites] = useState<InviteInfo[]>([]);
  const [inviteEmail, setInviteEmail] = useState("");
  const [error, setError] = useState<string | null>(null);

  const load = async () => {
    try {
      const [nextMembers, nextInvites] = await Promise.all([
        api.orgs.listOrgMembers(orgId),
        api.orgs.listOrgInvites(orgId).catch(() => []),
      ]);
      setMembers(nextMembers);
      setInvites(nextInvites);
      setError(null);
    } catch (caught) {
      setMembers([]);
      setError(asMessage(caught));
    }
  };

  useEffect(() => {
    void load();
  }, [orgId]);

  return (
    <div className={className}>
      <Notice error={error} />
      <Card>
        <CardHeader>
          <CardTitle>Members</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Member</TableHead>
                <TableHead>Role</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {members.length === 0 ? (
                <EmptyRow colSpan={3}>{emptyMessage}</EmptyRow>
              ) : (
                members.map((member) => (
                  <TableRow key={member.user_id}>
                    <TableCell>
                      <strong>{member.username}</strong>
                      <div>{member.email}</div>
                    </TableCell>
                    <TableCell>{member.role}</TableCell>
                    <TableCell>
                      {canManage && member.role !== "owner" ? (
                        <Button
                          aria-label={`Remove ${member.username}`}
                          onClick={async () => {
                            await api.orgs.removeOrgMember(orgId, member.user_id);
                            setMembers((current) =>
                              current.filter((item) => item.user_id !== member.user_id),
                            );
                            notify?.(`Removed ${member.username}.`, "success");
                          }}
                          variant="destructive"
                        >
                          Remove
                        </Button>
                      ) : null}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
        </CardContent>
      </Card>
      <Card>
        <CardHeader>
          <CardTitle>Invites</CardTitle>
        </CardHeader>
        <CardContent>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>Email</TableHead>
                <TableHead>Expires</TableHead>
                <TableHead />
              </TableRow>
            </TableHeader>
            <TableBody>
              {invites.length === 0 ? (
                <EmptyRow colSpan={3}>No pending invites.</EmptyRow>
              ) : (
                invites.map((invite) => (
                  <TableRow key={invite.id}>
                    <TableCell>{invite.email}</TableCell>
                    <TableCell>{formatDate(invite.expires_at)}</TableCell>
                    <TableCell>
                      {canManage ? (
                        <Button
                          aria-label={`Cancel invite for ${invite.email}`}
                          onClick={async () => {
                            await api.orgs.cancelInvite(orgId, invite.id);
                            setInvites((current) =>
                              current.filter((item) => item.id !== invite.id),
                            );
                            notify?.(`Cancelled invite for ${invite.email}.`, "success");
                          }}
                          variant="destructive"
                        >
                          Cancel
                        </Button>
                      ) : null}
                    </TableCell>
                  </TableRow>
                ))
              )}
            </TableBody>
          </Table>
          {canManage ? (
            <form className="ankh-inline-form" onSubmit={(event) => submitInvite(event)}>
              <Input
                id="org-invite-email"
                onChange={(event) => setInviteEmail(event.target.value)}
                placeholder="Email address"
                value={inviteEmail}
              />
              <Button id="org-invite-submit" type="submit">
                Invite
              </Button>
            </form>
          ) : null}
        </CardContent>
      </Card>
    </div>
  );

  async function submitInvite(event: FormEvent) {
    event.preventDefault();
    const emailError = validateEmail(inviteEmail);
    if (emailError) {
      setError(emailError);
      return;
    }
    try {
      const invite = await api.orgs.inviteToOrg(orgId, inviteEmail.trim().toLowerCase());
      setInvites((current) => [...current, invite]);
      setInviteEmail("");
      setError(null);
      notify?.(`Invited ${invite.email}.`, "success");
    } catch (caught) {
      setError(asMessage(caught));
    }
  }
}

export function DeviceSessionsPanel({
  className,
  emptyMessage = "No devices yet.",
  formatDate = defaultFormatDate,
  notify,
}: DeviceSessionsPanelProps) {
  const api = useAnkhApi();
  const [sessions, setSessions] = useState<DeviceSessionInfo[]>([]);

  useEffect(() => {
    void api.deviceSessions
      .listDeviceSessions()
      .then(setSessions)
      .catch(() => setSessions([]));
  }, [api]);

  return (
    <Card className={className}>
      <CardContent>
        <Table>
          <TableHeader>
            <TableRow>
              <TableHead>Device</TableHead>
              <TableHead>Platform</TableHead>
              <TableHead>Status</TableHead>
              <TableHead>Last used</TableHead>
              <TableHead />
            </TableRow>
          </TableHeader>
          <TableBody>
            {sessions.length === 0 ? (
              <EmptyRow colSpan={5}>{emptyMessage}</EmptyRow>
            ) : (
              sessions.map((session) => (
                <TableRow key={session.id}>
                  <TableCell>{session.device_name}</TableCell>
                  <TableCell>{session.platform}</TableCell>
                  <TableCell>{session.status}</TableCell>
                  <TableCell>{formatDate(session.last_used_at)}</TableCell>
                  <TableCell>
                    <Button
                      onClick={async () => {
                        await api.deviceSessions.revokeDeviceSession(session.id);
                        setSessions((current) => current.filter((item) => item.id !== session.id));
                        notify?.(`Revoked ${session.device_name}.`, "success");
                      }}
                      variant="destructive"
                    >
                      Revoke
                    </Button>
                  </TableCell>
                </TableRow>
              ))
            )}
          </TableBody>
        </Table>
      </CardContent>
    </Card>
  );
}

export function canManageOrg(org: OrgInfo) {
  return org.role === "owner" || org.role === "admin";
}

export function useOrg(id: string | null | undefined, enabled = true) {
  const api = useAnkhApi();
  const { setCurrentOrgId } = useCurrentOrg();
  const [org, setOrg] = useState<OrgInfo | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [isLoading, setIsLoading] = useState(false);

  useEffect(() => {
    if (!enabled || !id) {
      setOrg(null);
      setIsLoading(false);
      return;
    }
    setIsLoading(true);
    void api.orgs
      .getOrg(id)
      .then((nextOrg) => {
        setOrg(nextOrg);
        setCurrentOrgId(nextOrg.id);
        setError(null);
      })
      .catch((caught) => {
        setOrg(null);
        setError(asMessage(caught));
      })
      .finally(() => setIsLoading(false));
  }, [api, enabled, id, setCurrentOrgId]);

  return { error, isLoading, org };
}

function Notice({ error, success }: { error?: string | null; success?: string | null }) {
  if (error) {
    return <Alert variant="error">{error}</Alert>;
  }
  if (success) {
    return <Alert variant="success">{success}</Alert>;
  }
  return null;
}

function EmptyRow({ children, colSpan }: { children: ReactNode; colSpan: number }) {
  return (
    <TableRow>
      <TableCell colSpan={colSpan}>{children}</TableCell>
    </TableRow>
  );
}

function defaultResolveRedirect(fallback: string) {
  return (state: unknown) => {
    if (state && typeof state === "object" && "from" in state) {
      const from = (state as { from?: unknown }).from;
      if (typeof from === "string" && from && from !== "/login") {
        return from;
      }
    }
    return fallback;
  };
}

function defaultFormatDate(value: string) {
  return new Date(value).toLocaleString();
}
