'use client';

import { Box, HStack, Tabs, Text } from '@chakra-ui/react';
import { Download, Image as ImageIcon } from 'lucide-react';
import type { ViewerSource } from '@/lib/api/schema-types';

interface SvgViewerProps {
  sources: ViewerSource[];
}

export function SvgViewer({ sources }: SvgViewerProps) {
  const topSource = sources.find((s) => s.artifact_type === 'pcb_top_svg');
  const bottomSource = sources.find((s) => s.artifact_type === 'pcb_bottom_svg');

  const availableTabs = [
    ...(topSource ? [{ value: 'top', label: 'Top', source: topSource }] : []),
    ...(bottomSource ? [{ value: 'bottom', label: 'Bottom', source: bottomSource }] : []),
  ];

  if (availableTabs.length === 0) return null;

  return (
    <Box>
      <HStack gap={2} mb={2}>
        <ImageIcon size={16} />
        <Text fontSize='sm' fontWeight='medium'>
          PCB Preview
        </Text>
      </HStack>
      <Tabs.Root defaultValue={availableTabs[0].value}>
        <Tabs.List>
          {availableTabs.map((tab) => (
            <Tabs.Trigger key={tab.value} value={tab.value}>
              {tab.label}
            </Tabs.Trigger>
          ))}
        </Tabs.List>
        {availableTabs.map((tab) => (
          <Tabs.Content key={tab.value} value={tab.value}>
            <HStack gap={2} mb={2}>
              <a href={tab.source.url ?? undefined} target='_blank' rel='noopener noreferrer'>
                <HStack gap={1} color='blue.600' _hover={{ textDecoration: 'underline' }}>
                  <Download size={14} />
                  <Text fontSize='sm'>Download {tab.label} SVG</Text>
                </HStack>
              </a>
            </HStack>
            <Box borderWidth='1px' borderRadius='md' overflow='hidden'>
              <iframe
                src={tab.source.url ?? undefined}
                sandbox=''
                width='100%'
                height='500px'
                title={`PCB ${tab.label} Preview`}
                style={{ display: 'block' }}
              />
            </Box>
          </Tabs.Content>
        ))}
      </Tabs.Root>
    </Box>
  );
}
