'use client';

import { Box, Flex, Heading, Image, Text } from '@chakra-ui/react';
import { LogOut, User } from 'lucide-react';
import type { CurrentUser } from '@/lib/auth';
import { routes } from '@/lib/routes';

interface HeaderProps {
  user: CurrentUser | null;
}

export function Header({ user }: HeaderProps) {
  return (
    <Flex
      as='header'
      position='sticky'
      top={0}
      zIndex={10}
      h='16'
      px={6}
      align='center'
      justify='space-between'
      borderBottomWidth='1px'
      borderColor='gray.200'
      bg='white'
    >
      <Heading size='md' fontWeight='bold'>
        BoardFlow
      </Heading>

      {user && (
        <Flex align='center' gap={3}>
          <Flex align='center' gap={2}>
            {user.github_avatar_url ? (
              <Image
                src={user.github_avatar_url}
                alt={user.github_login}
                w={8}
                h={8}
                borderRadius='full'
              />
            ) : (
              <User size={20} />
            )}
            <Text fontSize='sm' fontWeight='medium'>
              {user.github_login}
            </Text>
          </Flex>
          <Box
            cursor='pointer'
            color='gray.500'
            _hover={{ color: 'gray.800' }}
            title='Logout'
            onClick={async () => {
              await fetch('/api/v1/auth/logout', { method: 'POST' });
              window.location.href = routes.login();
            }}
          >
            <LogOut size={18} />
          </Box>
        </Flex>
      )}
    </Flex>
  );
}
