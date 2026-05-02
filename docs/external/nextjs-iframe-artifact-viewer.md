# Next.js App Router での iframe Artifact Viewer パターン

## 要約

BoardFlowのArtifact Viewer（PDF/SVG/iBOM表示・ダウンロード）は、Server Component で viewer-sources API の URL を取得し、Client Component で iframe/embed 表示を行うパターンで実装する。URL が短命（expires_at 付き）であるため、Client Component 側で URL 期限管理と再取得ロジックを持つ必要がある。

## 確認した情報

### Next.js App Router の基本原則

- デフォルトは Server Component。`"use client"` 宣言で Client Component にオプトイン
- Server Component は Client Component をインポート・レンダリングできるが、逆は不可
- Server Component → Client Component へはシリアライズ可能な props のみ渡せる（URL文字列、ステータスなど）
- Client Component に渡す props は React Server Component Payload (RSC Payload) として送信される

### Artifact Viewer の設計パターン

#### 全体アーキテクチャ

```
Server Component (page.tsx)
  └─ fetch viewer-sources API (サーバーサイド、認証cookie付き)
  └─ viewer status / URL をシリアライズ可能な props として渡す
      └─ Client Component (ArtifactViewer)
          ├─ PDF/SVG: <iframe> or <embed> or <object>
          ├─ iBOM: <iframe sandbox="allow-scripts allow-same-origin">
          └─ Download: <a href="..." download>
```

#### Server Component 側 (page.tsx)

```tsx
// app/repositories/[repositoryId]/boards/[boardProjectId]/runs/[boardRunId]/page.tsx
// Server Component: viewer-sources API を呼んで結果を Client に渡す

export default async function BoardRunPage({ params }: Props) {
  const viewerSources = await fetchViewerSources(params.boardRunId);
  return <ArtifactViewer viewerSources={viewerSources} boardRunId={params.boardRunId} />;
}
```

#### Client Component 側 (ArtifactViewer)

```tsx
"use client";

interface ArtifactViewerProps {
  viewerSources: ViewerSourcesResponse;
  boardRunId: string;
}

export function ArtifactViewer({ viewerSources, boardRunId }: ArtifactViewerProps) {
  const [sources, setSources] = useState(viewerSources);
  
  // URL 期限管理：expires_at 前に再取得
  useEffect(() => {
    const expiresAt = new Date(sources.expires_at).getTime();
    const refreshAt = expiresAt - 5 * 60 * 1000; // 5分前に再取得
    const timeout = setTimeout(async () => {
      const newSources = await refreshViewerSources(boardRunId);
      setSources(newSources);
    }, Math.max(0, refreshAt - Date.now()));
    return () => clearTimeout(timeout);
  }, [sources.expires_at, boardRunId]);

  return (
    <Tabs>
      {sources.viewers.schematic?.status === "available" && (
        <PdfViewer url={sources.viewers.schematic.primary.url} />
      )}
      {sources.viewers.ibom?.status === "available" && (
        <IbomViewer iframeUrl={sources.viewers.ibom.iframe_url} />
      )}
      {/* ... */}
    </Tabs>
  );
}
```

### 各 Viewer 種別の実装パターン

#### PDF Viewer

```tsx
function PdfViewer({ url }: { url: string }) {
  return (
    <iframe
      src={url}
      style={{ width: "100%", height: "80vh", border: "none" }}
      title="Schematic PDF"
    />
  );
}
```

- ブラウザ内蔵のPDFビューアを利用
- `<embed>` や `<object>` も候補だが、`<iframe>` が最も互換性が高い
- sandbox 属性は PDF には不要（ブラウザ内蔵ビューアが処理する）

#### SVG Viewer

```tsx
function SvgViewer({ url, title }: { url: string; title: string }) {
  return (
    <iframe
      src={url}
      style={{ width: "100%", height: "80vh", border: "none" }}
      title={title}
    />
  );
}
```

- SVG も iframe で表示。ブラウザがネイティブレンダリング
- SVG内にスクリプトが含まれる可能性があるため、必要に応じて `sandbox=""` (空) を指定してスクリプト実行を完全ブロック可能

#### iBOM Viewer

```tsx
function IbomViewer({ iframeUrl }: { iframeUrl: string }) {
  return (
    <iframe
      src={iframeUrl}
      sandbox="allow-scripts allow-same-origin"
      style={{ width: "100%", height: "80vh", border: "none" }}
      title="Interactive BOM"
    />
  );
}
```

- sandbox の詳細は `iframe-sandbox-ibom.md` 参照

#### Download リンク

```tsx
function DownloadLink({ url, filename, type }: DownloadProps) {
  return (
    <a href={url} download={filename}>
      Download {type}
    </a>
  );
}
```

- `download` 属性はクロスオリジンでは無視される場合がある
- artifact proxy が `Content-Disposition: attachment; filename="..."` ヘッダを付与することで確実にダウンロードさせる

### URL 期限管理の設計

viewer-sources API の URL は短命（1時間）。以下の戦略が必要：

1. **初回取得**: Server Component で API 呼び出し → props として Client に渡す
2. **期限前更新**: Client Component で `expires_at` を監視し、期限の5分前にバックグラウンドで再取得
3. **再取得API**: Next.js の Route Handler (`/api/viewer-sources/[boardRunId]`) 経由で、Client Component から viewer-sources API を再呼び出し
4. **エラーハンドリング**: 再取得失敗時はユーザーにページリロードを促す

```tsx
// app/api/viewer-sources/[boardRunId]/route.ts
// Next.js Route Handler: クライアントからの再取得リクエストをプロキシ
export async function GET(request: Request, { params }: { params: { boardRunId: string } }) {
  // session cookie をバックエンド API に転送
  const response = await fetch(`${API_BASE}/api/v1/board-runs/${params.boardRunId}/viewer-sources`, {
    headers: { cookie: request.headers.get("cookie") ?? "" },
  });
  return Response.json(await response.json());
}
```

### Viewer ステータスのUI表示

各 viewer の `status` に応じた表示分岐：

| status | 表示 |
|---|---|
| `available` | プレビュー or ダウンロードリンクを表示 |
| `partial` | 利用可能なもののみ表示 + 欠損警告 |
| `missing` | 「生成されていません」メッセージ |
| `failed` | 「生成に失敗しました」エラー表示 |
| `skipped` | 「このプロジェクトでは対象外です」メッセージ |

## BoardFlow への示唆

### 推奨コンポーネント構成

```
components/
  artifact-viewer/
    ArtifactViewer.tsx        # "use client" - タブ + URL 期限管理
    PdfViewer.tsx             # "use client" - PDF iframe
    SvgViewer.tsx             # "use client" - SVG iframe  
    IbomViewer.tsx            # "use client" - iBOM sandboxed iframe
    DownloadSection.tsx       # "use client" - ダウンロードリンク群
    ViewerStatusBadge.tsx     # viewer status 表示
```

### キーポイント

1. **Server Component で API 呼び出し**: viewer-sources API は認証が必要なため、Server Component (cookie転送可能) で初回取得
2. **Client Component で iframe 管理**: iframe の `src` 属性変更、タブ切り替え、URL 更新は Client Component の責務
3. **Route Handler で再取得プロキシ**: Client Component からの URL 再取得は Next.js Route Handler 経由で backend API を呼ぶ
4. **viewer status に応じた分岐**: available / partial / missing / failed / skipped を UI で適切に表示

## 採用/不採用判断

**採用**: Server Component → Client Component の props 渡し + Route Handler による URL 再取得パターン

## 制約とpitfall

1. **iframe の CSP / CORS**: artifact domain からの応答に `X-Frame-Options` や `frame-ancestors` が適切に設定されていないと iframe 表示がブロックされる
2. **PDF のブラウザ内蔵ビューア**: Safari/iOS では iframe 内の PDF 表示に制限がある場合がある。フォールバックとしてダウンロードリンクを常に提供する
3. **SVG のスクリプト実行**: KiCad が生成する SVG にスクリプトが含まれる可能性は低いが、安全のため sandbox="" を検討
4. **URL 更新時の iframe リロード**: `src` を変更すると iframe がリロードされ、iBOM のスクロール位置やチェック状態が失われる。URL 更新頻度を最小化する
5. **Route Handler の認証**: Next.js Route Handler は cookie を自動転送しないため、明示的に cookie ヘッダを転送する必要がある
6. **Hydration mismatch**: Server Component で生成した HTML と Client Component の初回レンダリングが一致する必要がある。iframe の `src` が Server/Client で同一であること

## 未解決の疑問

- なし

## 参照URL

- Next.js Server and Client Components: https://nextjs.org/docs/app/building-your-application/rendering/server-components
- Next.js Route Handlers: https://nextjs.org/docs/app/building-your-application/routing/route-handlers
- React Server Components: https://react.dev/reference/rsc/server-components
