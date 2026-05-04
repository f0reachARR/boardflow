import { Box, Button, Heading, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { getCurrentUser } from '@/lib/auth';

export default async function LoginPage({
  searchParams,
}: {
  searchParams: Promise<{ redirect_to?: string }>;
}) {
  const user = await getCurrentUser();

  if (user) {
    redirect('/repositories');
  }

  const params = await searchParams;
  const redirectTo = params.redirect_to;
  const loginHref = redirectTo
    ? `/api/v1/auth/login?redirect_to=${encodeURIComponent(redirectTo)}`
    : '/api/v1/auth/login';

  return (
    <Box minH='100vh' display='flex' alignItems='center' justifyContent='center' bg='gray.50'>
      <VStack gap={6} p={8} bg='white' borderRadius='lg' shadow='md' maxW='sm' w='full'>
        <VStack gap={2}>
          <Heading size='xl'>BoardFlow</Heading>
          <Text color='gray.600' textAlign='center'>
            KiCad Board CI/CD Dashboard
          </Text>
        </VStack>
        <Link href={loginHref} style={{ width: '100%' }}>
          <Button colorPalette='gray' size='lg' w='full'>
            Sign in with GitHub
          </Button>
        </Link>
      </VStack>
    </Box>
  );
}
