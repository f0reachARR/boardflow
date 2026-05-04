import { Box, Button, Heading, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';

export default function LoginPage() {
  return (
    <Box minH='100vh' display='flex' alignItems='center' justifyContent='center' bg='gray.50'>
      <VStack gap={6} p={8} bg='white' borderRadius='lg' shadow='md' maxW='sm' w='full'>
        <VStack gap={2}>
          <Heading size='xl'>BoardFlow</Heading>
          <Text color='gray.600' textAlign='center'>
            KiCad Board CI/CD Dashboard
          </Text>
        </VStack>
        <Link href='/api/v1/auth/login' style={{ width: '100%' }}>
          <Button colorPalette='gray' size='lg' w='full'>
            Sign in with GitHub
          </Button>
        </Link>
      </VStack>
    </Box>
  );
}
