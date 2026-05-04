'use client';

import { Box, HStack, Text } from '@chakra-ui/react';
import { Eye } from 'lucide-react';

interface IbomViewerProps {
  iframeUrl: string;
}

export function IbomViewer({ iframeUrl }: IbomViewerProps) {
  return (
    <Box>
      <HStack gap={2} mb={2}>
        <Eye size={16} />
        <Text fontSize='sm' fontWeight='medium'>
          Interactive BOM
        </Text>
      </HStack>
      <Box borderWidth='1px' borderRadius='md' overflow='hidden'>
        <iframe
          src={iframeUrl}
          sandbox='allow-scripts allow-same-origin'
          width='100%'
          height='700px'
          title='Interactive BOM Viewer'
          style={{ display: 'block' }}
        />
      </Box>
    </Box>
  );
}
