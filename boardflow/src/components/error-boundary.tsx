"use client"

import { Box, Heading, Text, Button, VStack } from "@chakra-ui/react"
import { useQueryErrorResetBoundary } from "@tanstack/react-query"

export function ErrorUI({ error, reset }: { error: Error & { digest?: string }; reset: () => void }) {
  const { reset: resetQuery } = useQueryErrorResetBoundary()

  const handleReset = () => {
    resetQuery()
    reset()
  }

  return (
    <Box py={8}>
      <VStack gap={4}>
        <Heading size="md">Something went wrong</Heading>
        <Text color="gray.600">{error.message}</Text>
        <Button onClick={handleReset} variant="outline">
          Try again
        </Button>
      </VStack>
    </Box>
  )
}
