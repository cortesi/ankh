import { useAuth } from "@ankh/auth-react";
import { Alert, Button } from "@ankh/ui";
import { createContext, useContext, useState, type PropsWithChildren, type ReactNode } from "react";
import { Link, useNavigate } from "react-router-dom";

export type Notify = (message: string, variant?: "success" | "error") => void;

interface Toast {
  message: string;
  variant: "success" | "error";
}

const NotifyContext = createContext<Notify | null>(null);

/** Access the app-wide toast notifier. */
export function useNotify(): Notify {
  const notify = useContext(NotifyContext);
  if (!notify) {
    throw new Error("useNotify must be used within Layout");
  }
  return notify;
}

/** App chrome: a header with the signed-in identity and a transient toast region. */
export function Layout({ children }: PropsWithChildren) {
  const { isLoading, logout, user } = useAuth();
  const navigate = useNavigate();
  const [toast, setToast] = useState<Toast | null>(null);

  const notify: Notify = (message, variant = "success") => {
    setToast({ message, variant });
    window.setTimeout(() => setToast(null), 4000);
  };

  return (
    <NotifyContext.Provider value={notify}>
      <div className="demo-shell">
        <header className="demo-header">
          <Link className="demo-brand" to="/">
            Ankh Demo
          </Link>
          <nav className="demo-nav">
            {isLoading ? null : user ? (
              <>
                <span className="demo-user">{user.username}</span>
                <Button
                  onClick={async () => {
                    await logout();
                    navigate("/login");
                  }}
                  size="sm"
                  variant="ghost"
                >
                  Log out
                </Button>
              </>
            ) : (
              <>
                <Link to="/login">Log in</Link>
                <Link to="/signup">Sign up</Link>
              </>
            )}
          </nav>
        </header>
        {toast ? (
          <div className="demo-toast">
            <Alert variant={toast.variant}>{toast.message}</Alert>
          </div>
        ) : null}
        <main className="demo-main">{children}</main>
      </div>
    </NotifyContext.Provider>
  );
}

/** Centered single-column wrapper used by the auth pages. */
export function CenteredPage({ children }: { children: ReactNode }) {
  return <div className="demo-centered">{children}</div>;
}
