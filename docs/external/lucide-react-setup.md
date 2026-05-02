# lucide-react アイコンライブラリ導入

## 要約

`lucide-react` は tree-shakable な SVG アイコンライブラリ。1000+ のアイコンを提供し、使用したアイコンのみがバンドルに含まれる。Next.js / React プロジェクトで直接 import して使用する。

## 確認した情報

### インストール

```bash
npm install lucide-react
```

### 基本的な使用方法

```tsx
import { Camera, GitBranch, CheckCircle, AlertTriangle } from 'lucide-react';

const App = () => {
  return (
    <div>
      <Camera />
      <GitBranch size={24} color="blue" />
      <CheckCircle size={16} strokeWidth={2} />
      <AlertTriangle size={20} className="text-yellow-500" />
    </div>
  );
};
```

### カスタマイズ Props

| Prop | デフォルト | 説明 |
|---|---|---|
| `size` | 24 | アイコンの width/height (px) |
| `color` | `currentColor` | stroke の色 |
| `strokeWidth` | 2 | stroke の太さ |
| `className` | - | CSS クラス |
| `absoluteStrokeWidth` | false | サイズに関わらず固定 stroke 幅 |

### アイコン名の命名規則

- PascalCase で import: `import { FileText } from 'lucide-react'`
- アイコン一覧: https://lucide.dev/icons/

### Tree-shaking

各アイコンは個別にインポートされるため、使用しないアイコンはバンドルから除外される。名前付きインポートを使えば自動的に tree-shake される。

## BoardFlow への示唆

- 状態表示アイコン（成功、失敗、警告、スキップ等）に直接活用できる
- BoardFlow で使いそうなアイコン例:
  - `CheckCircle` / `XCircle` / `AlertTriangle` / `Clock` — Run ステータス
  - `FileText` / `Image` / `Download` — Artifact 種別・操作
  - `GitBranch` / `GitCommit` — Repository/Branch 表示
  - `ExternalLink` — GitHub Issue リンク
  - `LogOut` / `User` — 認証関連

## 採用判断

**採用**: `docs/technology.md` で決定済み。軽量で tree-shakable、Chakra UI との相性も良い。

## 制約と pitfall

1. **v1 への移行**: lucide-react v1 がリリースされている。v0 から移行する場合はアイコン名の変更に注意。
2. **Chakra UI の Icon との併用**: Chakra UI にも `Icon` コンポーネントがあるが、lucide-react のアイコンをそのまま JSX として使えるので、Chakra の `Icon` wrapper は必須ではない。
3. **動的インポート**: アイコンを文字列名で動的に選択する場合は、lucide-react の Dynamic Icon component を使う必要がある。

## 未解決の疑問

- なし（導入は単純）

## 参照URL

- https://lucide.dev/guide/react/getting-started
- https://lucide.dev/guide/installation
- https://lucide.dev/icons/
