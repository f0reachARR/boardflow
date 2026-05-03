import { Box, VStack, HStack, Skeleton } from "@chakra-ui/react"

export function RunDetailSkeleton() {
  return (
    <Box>
      <Skeleton height="20px" width="400px" mb={4} />
      <VStack align="stretch" gap={6}>
        <Box>
          <HStack gap={2} mb={1}>
            <Skeleton height="32px" width="150px" />
            <Skeleton height="24px" width="80px" />
          </HStack>
          <HStack gap={4}>
            <Skeleton height="16px" width="100px" />
            <Skeleton height="16px" width="80px" />
            <Skeleton height="16px" width="120px" />
          </HStack>
        </Box>
        <Box borderWidth="1px" borderRadius="md" p={4}>
          <VStack align="stretch" gap={3}>
            {Array.from({ length: 4 }).map((_, i) => (
              <HStack key={i} justify="space-between">
                <Skeleton height="16px" width="120px" />
                <Skeleton height="16px" width="150px" />
              </HStack>
            ))}
          </VStack>
        </Box>
        <Box>
          <Skeleton height="24px" width="120px" mb={4} />
          <Skeleton height="200px" width="100%" />
        </Box>
      </VStack>
    </Box>
  )
}
