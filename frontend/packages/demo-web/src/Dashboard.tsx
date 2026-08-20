import {
  canManageOrg,
  DeviceSessionsPanel,
  NewOrgForm,
  OrgMembersPanel,
  useAuth,
  useCurrentOrg,
  VerificationBanner,
} from "@ankh/auth-react";
import { Button, Card, CardContent, CardDescription, CardHeader, CardTitle } from "@ankh/ui";

import { useNotify } from "./Layout";

/** Authenticated landing page exercising the org, members, and device surfaces. */
export function Dashboard() {
  const notify = useNotify();
  const { user } = useAuth();
  const { currentOrg, orgs, setCurrentOrgId } = useCurrentOrg();

  return (
    <div className="demo-dashboard">
      {user && !user.email_verified ? <VerificationBanner notify={notify} /> : null}

      <Card>
        <CardHeader>
          <CardTitle>Account</CardTitle>
          <CardDescription>The identity returned by /api/v1/auth/me.</CardDescription>
        </CardHeader>
        <CardContent>
          <dl className="demo-detail">
            <dt>Username</dt>
            <dd>{user?.username}</dd>
            <dt>Email</dt>
            <dd>{user?.email}</dd>
            <dt>Verified</dt>
            <dd>{user?.email_verified ? "yes" : "no"}</dd>
          </dl>
        </CardContent>
      </Card>

      <Card>
        <CardHeader>
          <CardTitle>Organizations</CardTitle>
          <CardDescription>Switch the active organization.</CardDescription>
        </CardHeader>
        <CardContent>
          <div className="demo-org-switcher">
            {orgs.map((org) => (
              <Button
                key={org.id}
                onClick={() => setCurrentOrgId(org.id)}
                size="sm"
                variant={org.id === currentOrg?.id ? "primary" : "ghost"}
              >
                {org.display_name ?? org.name}
              </Button>
            ))}
          </div>
        </CardContent>
      </Card>

      <NewOrgForm
        notify={notify}
        onCreated={(org) => notify(`Switched to ${org.display_name ?? org.name}.`)}
      />

      {currentOrg ? (
        <section>
          <h2>Members of {currentOrg.display_name ?? currentOrg.name}</h2>
          <OrgMembersPanel
            canManage={canManageOrg(currentOrg)}
            notify={notify}
            orgId={currentOrg.id}
          />
        </section>
      ) : null}

      <section>
        <h2>Devices</h2>
        <DeviceSessionsPanel notify={notify} />
      </section>
    </div>
  );
}
