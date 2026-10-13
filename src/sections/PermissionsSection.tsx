import PermissionList, { usePermissions } from "../PermissionList";

export default function PermissionsSection() {
  const status = usePermissions();
  return (
    <section className="card">
      <h2>Permissions</h2>
      <PermissionList status={status} />
    </section>
  );
}
