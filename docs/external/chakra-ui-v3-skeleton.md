# Chakra UI v3 Skeleton コンポーネント

Issue: #65

## 要約

Chakra UI v3 (`@chakra-ui/react` ^3.35.0) は `Skeleton`, `SkeletonCircle`, `SkeletonText` の3コンポーネントを提供する。v2 から移植され、v3 では `loading` prop による表示切替と `variant` prop (`pulse` / `shine` / `none`) が追加された。

## 確認した情報

### 1. インポートと基本コンポーネント

```tsx
import { Skeleton, SkeletonCircle, SkeletonText } from "@chakra-ui/react"
```

| コンポーネント | 用途 |
|---|---|
| `Skeleton` | 汎用の矩形プレースホルダー。`height`, `width` で形状を指定 |
| `SkeletonCircle` | 円形プレースホルダー。アバターやアイコン向け。`size` で直径指定 |
| `SkeletonText` | テキスト行のプレースホルダー。`noOfLines` で行数を指定 |

### 2. Props

| Prop | デフォルト | 値 | 説明 |
|---|---|---|---|
| `loading` | `true` | `true` / `false` | `true` でスケルトン表示、`false` で子要素をフェードイン表示 |
| `variant` | `pulse` | `pulse` / `shine` / `none` | アニメーションの種類 |
| `colorPalette` | `gray` | Chakra のカラーパレット名 | スケルトンの色 |
| `as` | - | `React.ElementType` | 基底要素を変更 |
| `asChild` | - | `boolean` | 子要素に props をマージ |

`SkeletonText` 固有:
| Prop | 説明 |
|---|---|
| `noOfLines` | 表示するテキスト行の数 |

### 3. 使用パターン

#### フィード型スケルトン（リスト・カード用）

```tsx
<Stack gap="6" maxW="xs">
  <HStack width="full">
    <SkeletonCircle size="10" />
    <SkeletonText noOfLines={2} />
  </HStack>
  <Skeleton height="200px" />
</Stack>
```

#### `loading` prop による切替（条件付き表示）

```tsx
<Skeleton loading={isLoading}>
  <Text>実際のコンテンツ</Text>
</Skeleton>
```

`loading` が `false` になると子要素がフェードインで表示される。

#### テーブル用スケルトン

```tsx
<Table.Root>
  <Table.Header>
    <Table.Row>
      <Table.ColumnHeader>Name</Table.ColumnHeader>
      <Table.ColumnHeader>Status</Table.ColumnHeader>
    </Table.Row>
  </Table.Header>
  <Table.Body>
    {Array.from({ length: 5 }).map((_, i) => (
      <Table.Row key={i}>
        <Table.Cell>
          <Skeleton height="20px" />
        </Table.Cell>
        <Table.Cell>
          <Skeleton height="20px" width="80px" />
        </Table.Cell>
      </Table.Row>
    ))}
  </Table.Body>
</Table.Root>
```

#### カード型スケルトン

```tsx
<Box borderWidth="1px" borderRadius="lg" p={4}>
  <HStack mb={4}>
    <SkeletonCircle size="10" />
    <Stack flex="1">
      <SkeletonText noOfLines={1} />
      <SkeletonText noOfLines={1} />
    </Stack>
  </HStack>
  <Skeleton height="150px" />
</Box>
```

### 4. variant のアニメーション

- **`pulse`（デフォルト）**: 不透明度が点滅するパルスアニメーション
- **`shine`**: 左から右へ光が走るシマーアニメーション
- **`none`**: アニメーションなし（静的なプレースホルダー）

### 5. カスタムカラー

CSS 変数で開始色・終了色を変更可能:

```tsx
<Skeleton
  height="20px"
  css={{
    "--start-color": "colors.pink.100",
    "--end-color": "colors.pink.400",
  }}
/>
```

## BoardFlow への示唆

### 推奨するスケルトンUI

| 画面 | スケルトン構成 |
|---|---|
| リポジトリ一覧（テーブル） | `Table.Root` + 行ごとに `Skeleton` × カラム数 |
| ボードプロジェクト詳細 | カードレイアウトの各セクションに `Skeleton` + `SkeletonText` |
| ラン一覧 | テーブル行スケルトン |
| ラン詳細 | ヘッダー情報は `SkeletonText`、artifact グリッドは `Skeleton` |

### 実装方針

1. **再利用コンポーネントとして作成**: `components/skeleton/` に `TableSkeleton`, `CardSkeleton` 等の共通スケルトンを用意
2. **`loading` prop パターンを活用**: TanStack Query の `isLoading` 状態と組み合わせて `<Skeleton loading={isLoading}>` で実際のコンテンツをラップ
3. **`loading.tsx` ではシンプルなスケルトン**: ページ全体のスケルトンは `loading.tsx` に配置
4. **CLS 対策**: スケルトンの高さ・幅を実際のコンテンツに合わせて設計

## 採用/不採用判断

**採用**: Chakra UI v3 にビルトインで含まれており、追加パッケージ不要。`@chakra-ui/react` から直接インポート可能。

## 制約と pitfall

1. **Server Component でも使用可能**: Chakra UI v3 の Skeleton コンポーネントは Server Component からも利用できる。`loading.tsx` で直接 `<Skeleton>` を返すことが可能
2. **`loading.tsx` での使用**: `loading.tsx` はデフォルトで Server Component であり、Skeleton コンポーネントをそのまま使用できる
3. **`noOfLines` の高さ予測**: `SkeletonText` の `noOfLines` は実際のテキスト行数と一致させないと CLS が発生する
4. **アニメーションパフォーマンス**: `pulse` は CSS animation ベースで軽量。`shine` は若干重いが実用上問題なし

## 参照URL

- https://chakra-ui.com/docs/components/skeleton （公式 v3 ドキュメント）
- https://github.com/chakra-ui/chakra-ui/tree/main/packages/react/src/components/skeleton （ソースコード）
