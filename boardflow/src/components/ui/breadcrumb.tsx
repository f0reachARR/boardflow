import { Text } from "@chakra-ui/react"
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
    <nav aria-label="Breadcrumb" style={{ marginBottom: "var(--chakra-spacing-4)" }}>
      <ol
        style={{
          display: "flex",
          alignItems: "center",
          gap: "var(--chakra-spacing-1)",
          listStyle: "none",
          margin: 0,
          padding: 0,
          fontSize: "var(--chakra-fontSizes-sm)",
        }}
      >
        {items.map((item, index) => (
          <li key={index} style={{ display: "flex", alignItems: "center", gap: "var(--chakra-spacing-1)" }}>
            {index > 0 && <ChevronRight size={14} aria-hidden="true" color="var(--chakra-colors-gray-600)" />}
            {item.href ? (
              <Link href={item.href}>
                <Text color="blue.600" _hover={{ textDecoration: "underline" }}>
                  {item.label}
                </Text>
              </Link>
            ) : (
              <Text fontWeight="medium" color="gray.800" aria-current="page">
                {item.label}
              </Text>
            )}
          </li>
        ))}
      </ol>
    </nav>
  )
}
