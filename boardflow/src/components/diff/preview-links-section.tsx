'use client';

import { Box, Heading, HStack, Text, VStack } from '@chakra-ui/react';
import Link from 'next/link';
import { isRecord } from '@/lib/domain/guards';
import { shortId } from '@/lib/format';
import { routes } from '@/lib/routes';

export interface PreviewLinksSectionProps {
  metadata: Record<string, unknown> | null;
  repositoryId: string;
  boardProjectId: string;
  boardRunId: string;
  baseRunId: string | null;
}

export function PreviewLinksSection({
  metadata,
  repositoryId,
  boardProjectId,
  boardRunId,
  baseRunId,
}: PreviewLinksSectionProps) {
  const previewsRaw = metadata?.previews;
  const previews = isRecord(previewsRaw) ? previewsRaw : null;

  if (!previews) return null;

  const previewEntries = Object.entries(previews).filter(
    ([, value]) => typeof value === 'string' || isRecord(value),
  );

  if (previewEntries.length === 0) return null;

  const currentRunUrl = routes.run(repositoryId, boardProjectId, boardRunId);
  const baseRunUrl = baseRunId ? routes.run(repositoryId, boardProjectId, baseRunId) : null;

  return (
    <Box>
      <Heading size='md' mb={3}>
        Preview
      </Heading>
      <Box borderWidth='1px' borderRadius='md' p={4} bg='white'>
        <VStack align='stretch' gap={2}>
          <HStack gap={4} fontSize='sm'>
            <Text>
              Current run:{' '}
              <Link href={currentRunUrl}>
                <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  {shortId(boardRunId)}
                </Text>
              </Link>
            </Text>
            {baseRunUrl && baseRunId && (
              <Text>
                Base run:{' '}
                <Link href={baseRunUrl}>
                  <Text as='span' color='blue.600' _hover={{ textDecoration: 'underline' }}>
                    {shortId(baseRunId)}
                  </Text>
                </Link>
              </Text>
            )}
          </HStack>
          <Text fontSize='sm' fontWeight='bold' mt={1}>
            Available previews:
          </Text>
          {previewEntries.map(([type, value]) => (
            <HStack key={type} gap={2} fontSize='sm'>
              <Text fontFamily='mono' color='gray.700'>
                {type}
              </Text>
              {typeof value === 'string' && (
                <Text color='gray.500' truncate>
                  — {value}
                </Text>
              )}
              {isRecord(value) && typeof value.path === 'string' && (
                <Text color='gray.500' truncate>
                  — {value.path}
                </Text>
              )}
            </HStack>
          ))}
        </VStack>
      </Box>
    </Box>
  );
}
