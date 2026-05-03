"use client"

import { Box, Heading, Text, Button, VStack } from "@chakra-ui/react"

export function ErrorUI({ error, reset }: { error: Error & { digest?: string }; reset: () => void }) {
  return (
    <Box py={8}>
      <VStack gap={4}>
        <Heading size="md">Something went wrong</Heading>
        <Text color="gray.600">{error.message}</Text>
        <Button onClick={reset} variant="outline">
          Try again
        </Button>
      </VStack>
    </Box>
  )
}
