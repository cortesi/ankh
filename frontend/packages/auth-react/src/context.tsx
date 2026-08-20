import type { OrgInfo, UserInfo } from "@ankh/types";
import {
  createContext,
  startTransition,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type PropsWithChildren,
  type ReactNode,
} from "react";
import { Navigate, useLocation } from "react-router-dom";

import { Alert, Spinner } from "@ankh/ui";

import { type AnkhApi, createAnkhApi } from "./api";

export interface AuthContextValue {
  user: UserInfo | null;
  isLoading: boolean;
  refresh(): Promise<void>;
  login(email: string, password: string): Promise<UserInfo>;
  signup(
    username: string,
    email: string,
    password: string,
    inviteToken?: string | null,
    orgInviteToken?: string | null,
  ): Promise<UserInfo>;
  logout(): Promise<void>;
  clearAuthState(): void;
}

export interface CurrentOrgContextValue {
  orgs: OrgInfo[];
  currentOrgId: string | null;
  currentOrg: OrgInfo | null;
  personalOrg: OrgInfo | null;
  isLoading: boolean;
  reload(): Promise<void>;
  setCurrentOrgId(id: string | null): void;
}

export interface AnkhApiProviderProps extends PropsWithChildren {
  api?: AnkhApi;
}

export interface AuthProviderProps extends PropsWithChildren {
  api?: AnkhApi;
}

export interface CurrentOrgProviderProps extends PropsWithChildren {
  routeOrgId?: string | null;
  storageKey?: string | null;
}

export interface ProtectedProps {
  children: ReactNode;
  loadingFallback?: ReactNode;
  loginPath?: string;
}

const DEFAULT_STORAGE_KEY = "ankh.current-org-id";

const ApiContext = createContext<AnkhApi | null>(null);
const AuthContext = createContext<AuthContextValue | null>(null);
const CurrentOrgContext = createContext<CurrentOrgContextValue | null>(null);

export function AnkhApiProvider({ api = createAnkhApi(), children }: AnkhApiProviderProps) {
  return <ApiContext.Provider value={api}>{children}</ApiContext.Provider>;
}

export function useAnkhApi() {
  const api = useContext(ApiContext);
  if (!api) {
    throw new Error("useAnkhApi must be used within AnkhApiProvider");
  }
  return api;
}

export function AuthProvider({ api, children }: AuthProviderProps) {
  return (
    <AnkhApiProvider api={api}>
      <AuthStateProvider>{children}</AuthStateProvider>
    </AnkhApiProvider>
  );
}

function AuthStateProvider({ children }: PropsWithChildren) {
  const api = useAnkhApi();
  const [user, setUser] = useState<UserInfo | null>(null);
  const [isLoading, setIsLoading] = useState(true);

  const refresh = useCallback(async () => {
    try {
      const response = await api.auth.getCurrentUser();
      startTransition(() => {
        setUser(response.user);
        setIsLoading(false);
      });
    } catch {
      startTransition(() => {
        setUser(null);
        setIsLoading(false);
      });
    }
  }, [api]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const login = useCallback(
    async (email: string, password: string) => {
      const nextUser = await api.auth.login({ email, password });
      startTransition(() => {
        setUser(nextUser);
      });
      return nextUser;
    },
    [api],
  );

  const signup = useCallback(
    async (
      username: string,
      email: string,
      password: string,
      inviteToken?: string | null,
      orgInviteToken?: string | null,
    ) => {
      const nextUser = await api.auth.signup({
        username,
        email,
        password,
        invite_token: inviteToken ?? null,
        org_invite_token: orgInviteToken ?? null,
      });
      startTransition(() => {
        setUser(nextUser);
      });
      return nextUser;
    },
    [api],
  );

  const clearAuthState = useCallback(() => {
    startTransition(() => {
      setUser(null);
    });
  }, []);

  const logout = useCallback(async () => {
    try {
      await api.auth.logout();
    } finally {
      clearAuthState();
    }
  }, [api, clearAuthState]);

  const value = useMemo<AuthContextValue>(
    () => ({ user, isLoading, refresh, login, signup, logout, clearAuthState }),
    [user, isLoading, refresh, login, signup, logout, clearAuthState],
  );

  return <AuthContext.Provider value={value}>{children}</AuthContext.Provider>;
}

export function useAuth() {
  const context = useContext(AuthContext);
  if (!context) {
    throw new Error("useAuth must be used within AuthProvider");
  }
  return context;
}

export function CurrentOrgProvider({
  children,
  routeOrgId = null,
  storageKey = DEFAULT_STORAGE_KEY,
}: CurrentOrgProviderProps) {
  const api = useAnkhApi();
  const { user } = useAuth();
  const [orgs, setOrgs] = useState<OrgInfo[]>([]);
  const [currentOrgId, setCurrentOrgIdState] = useState<string | null>(() =>
    readStoredOrgId(storageKey),
  );
  const [isLoading, setIsLoading] = useState(false);

  const reload = useCallback(async () => {
    if (!user) {
      startTransition(() => {
        setOrgs([]);
        setCurrentOrgIdState(null);
        setIsLoading(false);
      });
      return;
    }

    setIsLoading(true);
    try {
      const nextOrgs = await api.orgs.listOrgs();
      startTransition(() => {
        setOrgs(nextOrgs);
        const existing = nextOrgs.find((org) => org.id === currentOrgId);
        const personalOrg = findPersonalOrg(nextOrgs);
        setCurrentOrgIdState(
          routeOrgId ?? existing?.id ?? personalOrg?.id ?? nextOrgs[0]?.id ?? null,
        );
        setIsLoading(false);
      });
    } catch {
      startTransition(() => {
        setOrgs([]);
        setCurrentOrgIdState(null);
        setIsLoading(false);
      });
    }
  }, [api, currentOrgId, routeOrgId, user]);

  useEffect(() => {
    void reload();
  }, [reload]);

  useEffect(() => {
    writeStoredOrgId(storageKey, currentOrgId);
  }, [currentOrgId, storageKey]);

  useEffect(() => {
    if (!routeOrgId || currentOrgId === routeOrgId) {
      return;
    }
    const routeOrg = orgs.find((org) => org.id === routeOrgId);
    if (routeOrg) {
      setCurrentOrgIdState(routeOrg.id);
    }
  }, [currentOrgId, orgs, routeOrgId]);

  const currentOrg = useMemo(
    () => orgs.find((org) => org.id === currentOrgId) ?? null,
    [currentOrgId, orgs],
  );
  const personalOrg = useMemo(() => findPersonalOrg(orgs), [orgs]);

  const value = useMemo<CurrentOrgContextValue>(
    () => ({
      orgs,
      currentOrgId,
      currentOrg,
      personalOrg,
      isLoading,
      reload,
      setCurrentOrgId: setCurrentOrgIdState,
    }),
    [orgs, currentOrgId, currentOrg, personalOrg, isLoading, reload],
  );

  return <CurrentOrgContext.Provider value={value}>{children}</CurrentOrgContext.Provider>;
}

export function useCurrentOrg() {
  const context = useContext(CurrentOrgContext);
  if (!context) {
    throw new Error("useCurrentOrg must be used within CurrentOrgProvider");
  }
  return context;
}

export function Protected({ children, loadingFallback, loginPath = "/login" }: ProtectedProps) {
  const { isLoading, user } = useAuth();
  const location = useLocation();

  if (isLoading) {
    return (
      loadingFallback ?? (
        <Alert>
          <Spinner /> Loading...
        </Alert>
      )
    );
  }
  if (!user) {
    return <Navigate replace state={{ from: location.pathname }} to={loginPath} />;
  }
  return <>{children}</>;
}

function readStoredOrgId(storageKey: string | null) {
  if (!storageKey) {
    return null;
  }
  const storage = browserStorage();
  return typeof storage?.getItem === "function" ? storage.getItem(storageKey) : null;
}

function writeStoredOrgId(storageKey: string | null, id: string | null) {
  if (!storageKey) {
    return;
  }
  const storage = browserStorage();
  if (typeof storage?.setItem !== "function" || typeof storage?.removeItem !== "function") {
    return;
  }

  if (id) {
    storage.setItem(storageKey, id);
  } else {
    storage.removeItem(storageKey);
  }
}

function browserStorage() {
  if (typeof window === "undefined") {
    return null;
  }
  return window.localStorage;
}

function findPersonalOrg(orgs: OrgInfo[]) {
  return orgs.find((org) => org.is_personal) ?? null;
}
