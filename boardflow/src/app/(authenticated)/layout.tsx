import { redirect } from "next/navigation"
import { getCurrentUser } from "@/lib/auth"
import { AppShell } from "@/components/layout/app-shell"

export default async function AuthenticatedLayout({
  children,
}: {
  children: React.ReactNode
}) {
  const user = await getCurrentUser()

  if (!user) {
    redirect("/login")
  }

  return <AppShell user={user}>{children}</AppShell>
}
