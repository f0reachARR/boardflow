"use client"

import { Box, VStack, Text } from "@chakra-ui/react"
import { FolderGit2 } from "lucide-react"
import Link from "next/link"
import { usePathname } from "next/navigation"

const NAV_ITEMS = [
  { href: "/repositories", label: "Repositories", icon: FolderGit2 },
]

export function Sidebar() {
  const pathname = usePathname()

  return (
    <Box
      as="nav"
      w="60"
      minH="calc(100vh - 4rem)"
      borderRightWidth="1px"
      borderColor="gray.200"
      bg="white"
      py={4}
      px={3}
    >
      <VStack align="stretch" gap={1}>
        {NAV_ITEMS.map((item) => {
          const isActive = pathname.startsWith(item.href)
          return (
            <Link key={item.href} href={item.href}>
              <Box
                display="flex"
                alignItems="center"
                gap={3}
                px={3}
                py={2}
                borderRadius="md"
                fontSize="sm"
                fontWeight="medium"
                color={isActive ? "blue.700" : "gray.700"}
                bg={isActive ? "blue.50" : "transparent"}
                _hover={{ bg: isActive ? "blue.50" : "gray.100" }}
              >
                <item.icon size={18} />
                <Text>{item.label}</Text>
              </Box>
            </Link>
          )
        })}
      </VStack>
    </Box>
  )
}
