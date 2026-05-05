import type { NextConfig } from 'next';

const nextConfig: NextConfig = {
  experimental: {
    optimizePackageImports: ['@chakra-ui/react'],
  },
  async rewrites() {
    return [
      {
        source: '/api/:path*',
        destination: `${process.env.API_BACKEND_URL ?? 'http://localhost:3000'}/api/:path*`,
      },
    ];
  },
  allowedDevOrigins: ['boardflow-dev.f0reach.me'],
};

export default nextConfig;
