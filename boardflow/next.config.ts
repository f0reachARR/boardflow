import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  output: 'standalone',
  experimental: {
    optimizePackageImports: ['@chakra-ui/react'],
  },
  allowedDevOrigins: ['boardflow-dev.f0reach.me'],
};

export default nextConfig;
