---
title: 画像生成プロンプト・チートシート（トーン/構図/照明ほか）
version: 1.0
updated: 2026-02-12
notes:
  - 「そのままコピペできる語彙集」を目的に、短い定義＋プロンプトで使える表現例を網羅的に整理。
  - 英語併記はモデルに伝わりやすいことが多い（推定）ため。日本語だけでも運用可能。
---

# 1. 使い方（最短）
- **重要度順**（効きやすい順の目安）：  
  **被写体 → 動作/関係 → 設定（場所/時間） → 構図 → 照明 → 色 → スタイル → 品質 → 制約**
- **書き方テンプレ**（短縮版）
  ```text
  被写体: {誰/何}。動作: {何をしている}。設定: {場所/時間/天気}。
  構図: {画角/アングル/配置}。照明: {光の種類/方向}。色: {パレット/温度}。
  スタイル: {媒体/画風}。品質: {高精細/質感}。制約: {入れない要素}。
  ```

---

# 2. トーン（雰囲気・感情・空気感）
**定義**：作品全体の「感情」「温度」「緊張感」「軽さ/重さ」を決める要素。

## 2.1 トーン語彙（プロンプト例）
| 日本語 | English | ひとこと定義 | 使いどころ |
|---|---|---|---|
| 明るく爽やか | bright, refreshing | 軽快・清潔感 | 広告/ポスター |
| 穏やか | calm, gentle | 緊張が低い | 日常/人物 |
| 叙情的 | poetic, lyrical | 感傷・余韻 | 夕景/回想 |
| ノスタルジック | nostalgic | 懐かしさ | 70s/80s風 |
| シネマティック | cinematic | 映画のよう | 予告編風 |
| ドラマチック | dramatic | 対比・強い感情 | ハイライト |
| 緊迫 | tense | 危機感 | 災害/対立 |
| ミステリアス | mysterious | 何かを隠す | 暗所/霧 |
| 不穏 | ominous | 嫌な予感 | ホラー寄り |
| ダーク | dark, moody | 低キーで重い | 夜/裏路地 |
| ミニマル | minimal | 情報が少ない | 製品/抽象 |
| ファンタジー | fantasy | 非現実 | 魔法/異世界 |
| レトロ | retro | 古い時代感 | フィルム/昭和 |
| 近未来 | futuristic | ハイテク感 | サイバー |
| ポップ | pop, playful | 明るい色・遊び | キャラ絵 |
| 上品 | elegant | 余白/落ち着き | 高級感 |
| かわいい | cute | 丸み・パステル | マスコット |
| かっこいい | cool | 直線・コントラスト | ヒーロー |

## 2.2 トーンを強めるコツ
- **低キー/高キー**：  
  - 低キー（dark, low-key）＝暗部多めで重い  
  - 高キー（high-key）＝全体明るく軽い
- **コントラスト**：high contrast / soft contrast
- **質感**：gritty（ざらつき）/ clean（清潔）

---

# 3. 構図（画面設計）
**定義**：被写体・背景・視線誘導の配置ルール。

## 3.1 ショットサイズ（距離感）
| 日本語 | English | 定義 |
|---|---|---|
| 超アップ | extreme close-up | 目/手など一部のみ |
| アップ | close-up | 顔〜肩 |
| バストショット | medium close-up | 胸上 |
| ミディアム | medium shot | 腰上 |
| フル | full shot | 全身 |
| ロング | long shot | 人物小さめ＋環境強調 |
| 超ロング | extreme long shot | 風景主体 |

## 3.2 アングル（視点）
| 日本語 | English | 印象 |
|---|---|---|
| 目線 | eye-level | 自然・日常 |
| 俯瞰 | high angle / top-down | 小さく/弱く見せる、説明向き |
| あおり | low angle | 強く/威圧、ヒーロー感 |
| 真俯瞰 | bird’s-eye view | 図解/俯瞰地図 |
| ローアングル手持ち | handheld low angle | ドキュメンタリー感（推定） |

## 3.3 構図ルール
| 日本語 | English | 使いどころ |
|---|---|---|
| 三分割 | rule of thirds | 迷ったらこれ |
| 中央構図 | centered composition | シンメトリ/威厳 |
| 対称構図 | symmetrical | 建築/正面 |
| 斜線構図 | diagonal composition | スピード/動き |
| 額縁構図 | frame within frame | 奥行き/集中 |
| リーディングライン | leading lines | 視線誘導 |
| 前景/中景/遠景 | foreground/midground/background | 立体感 |
| ネガティブスペース | negative space | 余白で高級感 |

## 3.4 レンズ感（画角の雰囲気）
| 表現 | English | 効果 |
|---|---|---|
| 広角（16–24mm風） | wide-angle | 迫力、歪み、空間広い |
| 標準（35–50mm風） | standard | 自然、汎用 |
| 望遠（85–135mm風） | telephoto | 圧縮効果、背景ボケ |
| 魚眼 | fisheye | 強い歪み、遊び |

---

# 4. 照明（ライティング）
**定義**：光源の種類・方向・硬さで立体感と感情を作る。

## 4.1 光の質（硬い/柔らかい）
| 日本語 | English | 特徴 |
|---|---|---|
| 柔らかい光 | soft light | 影が柔らかい、肌が綺麗 |
| 硬い光 | hard light | 影がくっきり、緊張感 |
| 拡散光 | diffused light | 曇天/大きい光源 |
| 点光源 | point light | 影が強い、夜間 |

## 4.2 光の方向
| 日本語 | English | 効果 |
|---|---|---|
| 順光 | front light | 情報が見える、影少なめ |
| 逆光 | backlight | シルエット/輪郭（リムライト） |
| サイド光 | side light | 立体感、ドラマチック |
| トップライト | top light | 上から、強い影（不穏にも） |
| アンダーライト | underlight | ホラー寄り |

## 4.3 ライティング定番
| 日本語 | English | ひとこと |
|---|---|---|
| レムブラント | Rembrandt lighting | 片側頬に三角光 |
| バタフライ | butterfly lighting | 鼻下に蝶の影、上品 |
| 3点照明 | three-point lighting | key/fill/rim の王道 |
| リムライト | rim light | 輪郭の光、主役強調 |
| ゴールデンアワー | golden hour | 夕方の暖色 |
| ブルーアワー | blue hour | 夕暮れの青 |

## 4.4 影とコントラスト
- 影を強める：deep shadows, hard shadow
- 影を弱める：soft shadows, minimal shadow
- コントラスト：high contrast / low contrast

---

# 5. 色（カラー設計）
**定義**：配色・色温度・彩度で印象を決める。

## 5.1 色温度（暖色/寒色）
| 日本語 | English | 印象 |
|---|---|---|
| 暖色 | warm color temperature | 安心・夕日 |
| 寒色 | cool color temperature | 静けさ・夜 |
| 中性 | neutral white balance | 製品/カタログ向き |

## 5.2 彩度・パレット
| 日本語 | English | 使いどころ |
|---|---|---|
| 高彩度 | vibrant, high saturation | ポップ/広告 |
| 低彩度 | muted, desaturated | シネマ/落ち着き |
| パステル | pastel palette | かわいい |
| モノクロ | monochrome | アート/硬派 |
| セピア | sepia tone | レトロ |
| ティール&オレンジ | teal and orange | 映画的対比 |

## 5.3 カラーグレーディング
- filmic color grading（映画風）
- clean color grading（清潔）
- vintage color grading（退色レトロ）

---

# 6. カメラ/描写（写真っぽさ・表現）
## 6.1 ピント・被写界深度
| 日本語 | English | 効果 |
|---|---|---|
| 浅い被写界深度 | shallow depth of field | 背景ボケ、主役強調 |
| 深い被写界深度 | deep depth of field | 全体くっきり、風景 |
| ボケ | bokeh | 光の玉ボケ（推定） |
| 背景ぼかし | background blur | 情報整理 |

## 6.2 ブレ・動き
| 日本語 | English | 効果 |
|---|---|---|
| モーションブラー | motion blur | スピード |
| 手ブレ感 | handheld | 臨場感（推定） |
| スローシャッター | long exposure | 光跡/流れ |

## 6.3 質感・素材表現
- metallic sheen（金属光沢）
- matte finish（マット塗装）
- glossy surface（光沢）
- rough texture（粗い）
- fabric weave（布目）

---

# 7. スタイル（媒体/画風）
| 日本語 | English | 例 |
|---|---|---|
| 写真 | photorealistic photo | studio photo, DSLR |
| 3Dレンダー | 3D render | octane render, unreal engine look（推定） |
| アニメ調 | anime style | clean lineart, cel shading |
| 水彩 | watercolor | soft wash, paper texture |
| 油彩 | oil painting | brush strokes, impasto |
| ペン画 | ink drawing | cross-hatching |
| ドット絵 | pixel art | 16-bit, limited palette |
| ミニマルフラット | flat design | vector, simple shapes |

---

# 8. 品質（仕上げ・破綻防止）
## 8.1 品質キーワード
- high detail / ultra-detailed
- sharp focus / crisp
- clean background
- realistic texture
- natural proportions（人物向け）

## 8.2 よくある制約（入れない）
| 目的 | 例（日本語） | 例（英語） |
|---|---|---|
| 文字を避ける | 文字なし、ロゴなし | no text, no logo |
| 透かし回避 | 透かしなし | no watermark |
| 人体破綻回避 | 余分な指なし、変形なし | no extra fingers, no deformed hands |
| 背景整理 | 背景ごちゃごちゃなし | no cluttered background |
| 低品質回避 | ぼやけなし、低解像度なし | not blurry, not low-res |

---

# 9. 用途別・一行プリセット（コピペ用）
## 9.1 製品カタログ
```text
studio product photo, centered composition, softbox lighting, neutral white balance, clean background, sharp details, no text, no watermark
```

## 9.2 映画予告風
```text
cinematic, dramatic lighting, high contrast, teal and orange color grading, shallow depth of field, subtle film grain
```

## 9.3 かわいいポップ
```text
cute, pastel palette, soft light, minimal background, clean lineart, playful composition
```

## 9.4 工場・教育スライド向け（視認性重視）
```text
clear instructional scene, eye-level, bright even lighting, deep depth of field, uncluttered background, realistic materials, no text overlays
```

---

# 10. チェックリスト（提出前の自己点検）
- 主役は1つに絞れているか（被写体が曖昧だと崩れやすい）
- 構図（距離・アングル・配置）が書けているか
- 光（種類・方向・硬さ）と色温度が一致しているか
- 制約（文字/透かし/破綻）を明示したか
- 目的（カタログ/ポスター/コンセプトアート）が先頭にあるか
