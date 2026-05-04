import { cookies } from 'next/headers';

export interface CurrentUser {
  user_id: string;
  github_login: string;
  github_avatar_url: string | null;
}

export async function getCurrentUser(): Promise<CurrentUser | null> {
  const cookieStore = await cookies();
  const session = cookieStore.get('boardflow_session');

  if (!session) {
    return null;
  }

  try {
    const res = await fetch(
      `${process.env.API_BASE_URL ?? 'http://localhost:3000'}/api/v1/auth/me`,
      {
        headers: {
          Cookie: `boardflow_session=${session.value}`,
        },
        cache: 'no-store',
      },
    );

    if (!res.ok) {
      return null;
    }

    return res.json();
  } catch {
    return null;
  }
}
