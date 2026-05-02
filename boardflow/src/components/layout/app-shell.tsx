import { Box, Flex } from "@chakra-ui/react"
import { Header } from "./header"
import { Sidebar } from "./sidebar"
import type { CurrentUser } from "@/lib/auth"

interface AppShellProps {
  user: CurrentUser | null
  children: React.ReactNode
}

export function AppShell({ user, children }: AppShellProps) {
  return (
    <Box minH="100vh" bg="gray.50">
      <Header user={user} />
      <Flex>
        <Sidebar />
        <Box as="main" flex={1} p={6}>
          {children}
        </Box>
      </Flex>
    </Box>
  )
}
