# iframe sandbox属性：iBOM HTML表示のベストプラクティス

## 要約

iBOM (InteractiveHtmlBom) は自己完結型のHTMLファイルで、JavaScript（Canvas描画、BOM操作）とlocalStorage（チェックボックス状態保存）を使用する。BoardFlowでは artifact domain（app domain とは別ドメイン）上で配信するため、iframe sandbox の `allow-scripts` と `allow-same-origin` を**同時に指定しても安全**だが、CSP レスポンスヘッダとの多層防御を推奨する。

## 確認した情報

### iBOM の JavaScript 実行要件

iBOM HTMLの実際のソースコード（`InteractiveHtmlBom/web/util.js`）を確認した結果：

- **JavaScript 必須**: Canvas/WebGL描画、BOMテーブル操作、ハイライト、チェックボックス状態管理すべてにJSが必要
- **localStorage 使用**: チェックボックス状態、レイアウト設定、ボード回転などをlocalStorageに保存。ただし graceful degradation 実装済み（localStorage不可時は sessionStorage → null にフォールバック）
- **外部ネットワーク通信なし**: 自己完結型HTML。外部へのfetch/XHRは行わない
- **フォーム送信なし**: BOMデータのクリップボードコピーやファイルダウンロードのみ
- **ポップアップなし**: `window.open` は使用しない

### sandbox 属性の選択肢と分析

#### 選択肢 1: `sandbox="allow-scripts"` (allow-same-origin なし)

```html
<iframe
  src="https://artifacts.boardflow.example.com/proxy/artifacts/art_ibom?token=..."
  sandbox="allow-scripts"
/>
```

- iBOM のJSは実行される
- **opaque origin になる**: localStorage / sessionStorage へのアクセスが`SecurityError`で失敗
- iBOM の `initStorage()` は localStorage/sessionStorage 両方のtry-catchでgraceful degradation するため、**設定保存はできないが表示・操作自体は動作する**
- iBOM内からparent frameへのアクセスは完全にブロック
- 最もセキュアな選択肢

#### 選択肢 2: `sandbox="allow-scripts allow-same-origin"` (同一ドメインの場合は危険)

```html
<iframe
  src="https://artifacts.boardflow.example.com/proxy/artifacts/art_ibom?token=..."
  sandbox="allow-scripts allow-same-origin"
/>
```

- iBOM のJSが実行され、localStorageも使用可能
- **W3C/WHATWG は同一オリジンの場合にこの組み合わせを非推奨**: iframe内のスクリプトがsandbox属性自体を除去できるため
- **ただし BoardFlow ではクロスオリジン配信**: app domain (`app.boardflow.example.com`) と artifact domain (`artifacts.boardflow.example.com`) が別ドメインなので、same-origin policy により parent frame へのアクセスは不可。sandbox 除去攻撃は成立しない
- localStorage へのアクセスは artifact domain のストレージに限定される（app domain の cookie/session には触れない）

#### 選択肢 3: sandbox なし + CSP ヘッダのみで制御

```html
<iframe src="https://artifacts.boardflow.example.com/proxy/artifacts/art_ibom?token=..." />
```

- artifact proxy が CSP レスポンスヘッダで制約を付与する方式
- sandbox属性によるブラウザ側保護がない

### allow-scripts + allow-same-origin が「安全でない」とされるケース

- **同一ドメイン**: iframe内のスクリプトが `parent.document.querySelector('iframe').removeAttribute('sandbox')` でsandbox自体を除去できる → 全制約解除
- **クロスオリジン**: same-origin policy により parent frame の DOM 操作不可。sandbox 除去は不可能。allow-same-origin は iframe 自身のオリジンストレージへのアクセスのみを許可する意味になる

### サーバー側 CSP レスポンスヘッダ（多層防御）

artifact proxy が iBOM HTML を配信する際の推奨 CSP ヘッダ：

```
Content-Security-Policy: default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data: blob:; frame-ancestors https://app.boardflow.example.com; sandbox allow-scripts allow-same-origin;
X-Content-Type-Options: nosniff
X-Frame-Options: ALLOW-FROM https://app.boardflow.example.com
```

- `frame-ancestors`: app domain からのみ iframe 埋め込みを許可
- `default-src 'none'`: 外部リソース読み込みをすべてブロック
- `script-src 'unsafe-inline'`: iBOMのインラインスクリプトのみ許可（外部スクリプト読み込みはブロック）
- `sandbox allow-scripts allow-same-origin`: CSPレベルでもsandbox制約を付与

## BoardFlow への示唆

### 推奨構成

1. **iframe sandbox 属性**: `sandbox="allow-scripts allow-same-origin"`
   - iBOMの全機能（localStorage含む）が動作する
   - クロスオリジン配信のため sandbox 除去攻撃は成立しない
2. **サーバー側 CSP ヘッダ**: 多層防御として artifact proxy で CSP を付与
3. **フォールバック**: iBOMが表示できない場合は BOM CSV のダウンロードリンクを提示

### 代替案（よりセキュア）

`sandbox="allow-scripts"` のみを使用し、localStorage による設定保存を諦める。iBOMの表示・操作には影響なし。ユーザーがページをリロードすると設定がリセットされるだけ。

## 採用/不採用判断

**採用**: `sandbox="allow-scripts allow-same-origin"` + サーバー側 CSP ヘッダの多層防御

理由：
- クロスオリジン配信が仕様で確定しているため、allow-scripts + allow-same-origin の危険な組み合わせのリスクは軽減される
- iBOMのlocalStorageによる設定保存機能が利用可能になり、UXが向上する
- サーバー側CSPで外部リソース読み込みとframe-ancestorsを制限し、多層防御を実現

## 制約とpitfall

1. **artifact domain が app domain と同一ドメインになった場合は危険**: 必ずクロスオリジンを維持すること
2. **iBOM が将来的に外部リソースを読み込むようになった場合**: CSP の `default-src 'none'` がブロックする。その場合は CSP を調整する必要がある
3. **localStorage の容量**: artifact domain のlocalStorageに複数プロジェクトの設定が蓄積される。各プロジェクトごとにprefixで区別されるため衝突は起きないが、容量上限（通常5MB）は意識する
4. **token 期限切れ**: iframe 内の iBOM は自己完結型のため token 期限切れの影響は受けないが、ページリロード時に iframe src の token が期限切れの場合は viewer-sources API を再呼び出しして新しい URL を取得する必要がある

## 未解決の疑問

- なし（クロスオリジン配信が仕様で確定しているため、主要な懸念は解消済み）

## 参照URL

- MDN - iframe sandbox: https://developer.mozilla.org/en-US/docs/Web/HTML/Element/iframe#sandbox
- MDN - CSP sandbox directive: https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Content-Security-Policy/sandbox
- W3C - Play safely in sandboxed IFrames: https://web.dev/articles/sandboxed-iframes
- StackOverflow - allow-scripts + allow-same-origin safety: https://stackoverflow.com/questions/35208161/is-it-safe-to-have-sandbox-allow-scripts-allow-popups-allow-same-origin
- InteractiveHtmlBom ソースコード (util.js): https://github.com/openscopeproject/InteractiveHtmlBom/blob/master/InteractiveHtmlBom/web/util.js
- CSP frame-ancestors: https://content-security-policy.com/frame-ancestors/
