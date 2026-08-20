import type {
  AuthMeResponse,
  CreateOrgInput,
  DeviceSessionInfo,
  InviteInfo,
  LoginRequest,
  MemberInfo,
  OrgInfo,
  OrgInviteDetails,
  OrgMemberInfo,
  SignupRequest,
  UserInfo,
  WaitlistStatusResponse,
} from "@ankh/types";

export class ApiError extends Error {
  status: number;
  code: string | null;

  constructor(status: number, message: string, code: string | null = null) {
    super(message);
    this.name = "ApiError";
    this.status = status;
    this.code = code;
  }
}

interface ErrorPayload {
  error?: {
    code?: string;
    message?: string;
  };
}

export type JsonRequest = <T>(path: string, init?: RequestInit) => Promise<T>;

export interface AnkhAuthApi {
  signup(request: SignupRequest): Promise<UserInfo>;
  login(request: LoginRequest): Promise<UserInfo>;
  logout(): Promise<void>;
  getCurrentUser(): Promise<AuthMeResponse>;
  getWaitlistStatus(): Promise<boolean>;
  verifyEmail(token: string): Promise<void>;
  resendVerification(): Promise<void>;
  requestPasswordReset(email: string): Promise<void>;
  validateResetToken(token: string): Promise<boolean>;
  resetPassword(token: string, newPassword: string): Promise<void>;
}

export interface AnkhOrgApi {
  listOrgs(): Promise<OrgInfo[]>;
  createOrg(input: CreateOrgInput): Promise<OrgInfo>;
  getOrg(id: string): Promise<OrgInfo>;
  getMyMembership(id: string): Promise<OrgMemberInfo>;
  leaveOrg(id: string): Promise<void>;
  listOrgMembers(id: string): Promise<MemberInfo[]>;
  removeOrgMember(orgId: string, memberId: string): Promise<void>;
  listOrgInvites(id: string): Promise<InviteInfo[]>;
  inviteToOrg(id: string, inviteEmail: string): Promise<InviteInfo>;
  cancelInvite(orgId: string, inviteId: string): Promise<void>;
  getOrgInviteDetails(token: string): Promise<OrgInviteDetails>;
  acceptOrgInvite(token: string): Promise<OrgInfo>;
}

export interface AnkhDeviceSessionApi {
  listDeviceSessions(): Promise<DeviceSessionInfo[]>;
  revokeDeviceSession(id: string): Promise<void>;
}

export interface AnkhApi {
  auth: AnkhAuthApi;
  orgs: AnkhOrgApi;
  deviceSessions: AnkhDeviceSessionApi;
}

export function createJsonRequest(fetchImpl: typeof fetch = fetch): JsonRequest {
  return async function jsonRequest<T>(path: string, init: RequestInit = {}): Promise<T> {
    const response = await fetchImpl(path, {
      credentials: "same-origin",
      headers: {
        "Content-Type": "application/json",
        ...init.headers,
      },
      ...init,
    });

    if (!response.ok) {
      const payload = (await safeJson<ErrorPayload>(response)) ?? {};
      throw new ApiError(
        response.status,
        payload.error?.message ?? `Request failed with ${response.status}`,
        payload.error?.code ?? null,
      );
    }

    if (response.status === 204) {
      return undefined as T;
    }

    return (await safeJson<T>(response)) as T;
  };
}

export function createAnkhApi(request: JsonRequest = createJsonRequest()): AnkhApi {
  return {
    auth: {
      signup: (body) => post<UserInfo>(request, "/api/v1/auth/signup", body),
      login: (body) => post<UserInfo>(request, "/api/v1/auth/login", body),
      logout: () => post<void>(request, "/api/v1/auth/logout", {}),
      getCurrentUser: () => request<AuthMeResponse>("/api/v1/auth/me"),
      getWaitlistStatus: async () => {
        const response = await request<boolean | WaitlistStatusResponse>(
          "/api/v1/auth/waitlist-status",
        );
        return typeof response === "boolean" ? response : response.waitlist_enabled;
      },
      verifyEmail: (token) => post<void>(request, "/api/v1/auth/verify-email", { token }),
      resendVerification: () => post<void>(request, "/api/v1/auth/resend-verification", {}),
      requestPasswordReset: (email) =>
        post<void>(request, "/api/v1/auth/forgot-password", { email }),
      validateResetToken: (token) =>
        post<boolean>(request, "/api/v1/auth/validate-reset-token", { token }),
      resetPassword: (token, newPassword) =>
        post<void>(request, "/api/v1/auth/reset-password", {
          token,
          new_password: newPassword,
        }),
    },
    orgs: {
      listOrgs: () => request<OrgInfo[]>("/api/v1/orgs"),
      createOrg: (body) => post<OrgInfo>(request, "/api/v1/orgs", body),
      getOrg: (id) => request<OrgInfo>(`/api/v1/orgs/${encodeURIComponent(id)}`),
      getMyMembership: (id) =>
        request<MemberInfo>(`/api/v1/orgs/${encodeURIComponent(id)}/membership`),
      leaveOrg: (id) => post<void>(request, `/api/v1/orgs/${encodeURIComponent(id)}/leave`, {}),
      listOrgMembers: (id) =>
        request<MemberInfo[]>(`/api/v1/orgs/${encodeURIComponent(id)}/members`),
      removeOrgMember: (orgId, memberId) =>
        request<void>(
          `/api/v1/orgs/${encodeURIComponent(orgId)}/members/${encodeURIComponent(memberId)}`,
          { method: "DELETE" },
        ),
      listOrgInvites: (id) =>
        request<InviteInfo[]>(`/api/v1/orgs/${encodeURIComponent(id)}/invites`),
      inviteToOrg: (id, invite_email) =>
        post<InviteInfo>(request, `/api/v1/orgs/${encodeURIComponent(id)}/invites`, {
          invite_email,
        }),
      cancelInvite: (orgId, inviteId) =>
        request<void>(
          `/api/v1/orgs/${encodeURIComponent(orgId)}/invites/${encodeURIComponent(inviteId)}`,
          { method: "DELETE" },
        ),
      getOrgInviteDetails: (token) =>
        request<OrgInviteDetails>(`/api/v1/org-invites/${encodeURIComponent(token)}`),
      acceptOrgInvite: (token) =>
        post<OrgInfo>(request, `/api/v1/org-invites/${encodeURIComponent(token)}/accept`, {}),
    },
    deviceSessions: {
      listDeviceSessions: () => request<DeviceSessionInfo[]>("/api/v1/device-sessions"),
      revokeDeviceSession: (id) =>
        request<void>(`/api/v1/device-sessions/${encodeURIComponent(id)}`, {
          method: "DELETE",
        }),
    },
  };
}

export function asMessage(caught: unknown) {
  if (caught instanceof Error) {
    return caught.message;
  }
  return "Something went wrong";
}

function post<T>(request: JsonRequest, path: string, body: unknown) {
  return request<T>(path, {
    method: "POST",
    body: JSON.stringify(body),
  });
}

async function safeJson<T>(response: Response): Promise<T | null> {
  const text = await response.text();
  if (!text) {
    return null;
  }
  return JSON.parse(text) as T;
}
