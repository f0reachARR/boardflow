import { HStack, Text } from "@chakra-ui/react"
import Link from "next/link"
import { ChevronRight } from "lucide-react"

export interface BreadcrumbItem {
  label: string
  href?: string
}

interface BreadcrumbProps {
  items: BreadcrumbItem[]
}

export function Breadcrumb({ items }: BreadcrumbProps) {
  return (
    <HStack gap={1} fontSize="sm" color="gray.600" mb={4}>
      {items.map((item, index) => (
        <HStack key={index} gap={1}>
          {index > 0 && <ChevronRight size={14} />}
          {item.href ? (
            <Link href={item.href}>
              <Text color="blue.600" _hover={{ textDecoration: "underline" }}>
                {item.label}
              </Text>
            </Link>
          ) : (
            <Text fontWeight="medium" color="gray.800">
              {item.label}
            </Text>
          )}
        </HStack>
      ))}
    </HStack>
  )
}
