# Figma -> Design IR -> GPUI Rust Compiler

**Durum:** Önerilen mimari ve araştırma sentezi
**Araştırma tarihi:** 2026-08-25
**Kapsam:** Yapısal Figma verisinden deterministik bir Design IR ve GPUI Rust kodu üretmek.

Bu belge screenshot-to-code tasarlamaz. Ekran görüntüsü yalnızca üretilen GPUI görünümünü
doğrulamak için referans ve regresyon girdisidir. Anima, Locofy veya kapalı Builder
ürünlerinin iç mimarisi hakkında kamuya açık kaynakla doğrulanamayan iddialar bu karara
dahil değildir.

## Özet Karar

1. `core-ir`, `normalize` ve `resolve` katmanları Figma SDK'sından, Code Connect'ten ve
   GPUI çalışma zamanından bağımsız kalır.
2. Figma REST/Plugin verisi bir giriş bağdaştırıcısı ile alınır; doğrudan codegen'e
   aktarılmaz.
3. Code Connect, gerçek kod component'leri ile Figma component/property eşlemesini
   sağlayan isteğe bağlı metadata bağdaştırıcısıdır. Core IR veya GPUI codegen bağımlılığı
   değildir.
4. Kod üretimi string birleştirme yerine Rust token/AST akışıyla yapılır:
   `proc-macro2` -> `quote` -> `syn` -> `prettyplease`.
5. Kabul kapısı üç kanıtı birlikte ister: IR/hiyerarşi, geometri ve görsel metrikler.
6. Screenshot-to-code, prototip davranışı ve bilinmeyen Figma özelliklerinin sessizce
   tahmini core kapsamına alınmaz; bunlar diagnostic üretir veya açık bağdaştırıcı ister.

## Mimari Akış

```text
 Figma REST / Plugin snapshot / checked-in fixture
                         |
                         v
              input adapter + schema validation
                         |
                         v
                canonicalization + diagnostics
                         |
                         v
          Design IR (IDs, hierarchy, layout, paint, props)
                         |
                         v
        reference resolver + component/variant resolution
                         |
              +----------+-----------+
              |                      |
              v                      v
       GPUI semantic lowering   visual fixture manifest
              |                      |
              v                      v
       Rust AST/token emitter    GPUI render capture
              |                      |
              v                      v
       deterministic .rs      geometry + pixel + SSIM report
```

Core ile dış dünya arasındaki sınırlar:

| Katman | Sorumluluk | Dış bağımlılık | Karar |
|---|---|---|---|
| `core-ir` | Node, layout, paint, component, prop ve diagnostic türleri | Yok | Zorunlu çekirdek |
| `normalize` | Varsayılanları, sayıları, renkleri ve sıraları kanonikleştirmek | Yok | Zorunlu çekirdek |
| `resolve` | Component, variant, token, asset ve mapping çözümlemek | Yok | Zorunlu çekirdek |
| `input-figma` | REST veya Plugin snapshot'ını IR girdisine çevirmek | Figma API/JSON sınırı | İsteğe bağlı bağdaştırıcı |
| `input-code-connect` | Code Connect eşlemelerini statik metadata'ya çevirmek | Code Connect CLI/template formatı | İsteğe bağlı bağdaştırıcı |
| `codegen-gpui` | IR'ı GPUI Rust modüllerine düşürmek | GPUI hedef sürümü ve AST crate'leri | Hedef backend |
| `visual` | Render, fixture, geometri ve görsel karşılaştırma | GPUI render ortamı ve PNG okuyucu | Geliştirme/test sınırı |
| `cli` | Snapshot alma, compile, diff ve raporlama | İşletim sistemi | İnce orchestration katmanı |

"Bağımlılıksız compiler" burada **core IR ve semantic compiler'ın platform/proprietary
araçlardan bağımsız olması** anlamındadır. Rust kaynak üretimi ve PNG karşılaştırması
doğal olarak kenar bağımlılıklarıdır; bunlar core'a taşınmaz.

## Kaynak Kanıtı

| Kaynak | Doğrulanan nokta | Mimari sonucu |
|---|---|---|
| [Figma REST giriş][s1] | Dosya ve layer/object özellikleri JSON olarak alınabilir. | REST snapshot input adapter ile sınırlanır. |
| [Figma file endpoints][s2] | `/v1/files/:key` ve `/v1/files/:key/nodes`; `ids`, `depth`, `geometry`, `version` seçenekleri vardır. | Büyük dosya için seçili subtree ve sürüm pinleme desteklenir. |
| [Figma node types][s3] | Node ağacı, absolute bounds, auto-layout, fills, strokes, text, component ve instance alanları ayrıdır. | IR layout/paint/content/component alanlarını ayrı tutar. |
| [Figma property types][s4] | Transform, paint, text style, component property ve variable alias türleri tanımlıdır. | Ham REST şemasını doğrudan GPUI API'sine bağlamamak gerekir. |
| [Figma Plugin API][s5] | Plugin'ler node ağacını ve hiyerarşiyi okuyabilir; sayfalar dinamik yüklendiği için erişim asenkron olabilir. | Plugin snapshot'ı önce tamamlanmış, sonra derlenebilir fixture olmalıdır. |
| [Code Connect introduction][s6] | Code Connect, Figma Dev Mode ile gerçek code component'leri ve üretim snippet'lerini bağlar. | Codegen'in yerine geçmez; mapping metadata sağlar. |
| [Code Connect templates][s7] | Template dosyaları framework-agnostic, property/slot/nested instance erişimi olan önerilen formattır. | Yeni adapter template formatını hedefler. |
| [Code Connect migration][s8] | Framework-specific parser'lar 2026-08-17 itibarıyla aktif güncellenmiyor. | Legacy parser API'sine yeni core bağımlılığı eklenmez. |
| [Mitosis node IR][s9] | `name`, `properties`, `bindings`, `children`, `slots`, `For` ve `Show` gibi abstract node kavramları vardır. | Component/node ayrımı için fikir kaynağıdır; kodu vend edilmez. |
| [Teleport UIDL][s10] | UIDL, UI'ı framework ve web platformundan bağımsız ifade edip generator'a verir. | Input -> abstract IR -> backend ayrımı doğrulanır. |
| [FigmaToCode conversion][s11] | Selection traversal, node limitleri, warning ve ağır önizleme kapatma gibi operasyonel korumalar kullanır. | Guardrail yaklaşımı alınır; GPL kodu alınmaz. |
| [FigmaToCode license][s12] | Repository GPL-3.0 metni taşır. | Proprietary core'a kaynak kodu kopyalanmaz. |
| [Figma SDS][s13] | Variables, styles, components ve Code Connect'i birlikte örnekler; URL substitution kullanır. | Token ve file-agnostic mapping fixture'ları için referanstır. |
| [proc-macro2][s14] | Token tabanlı üretimi procedural macro dışındaki `main.rs`/`build.rs` bağlamlarına taşır. | Codegen test edilebilir bir normal Rust crate olabilir. |
| [quote][s15] | `quote!` token üretir ve non-macro generator için `prettyplease` önerilir. | String concat yerine token üretimi seçilir. |
| [syn][s16] | Rust token stream'ini syntax tree'ye parse eder. | Üretim sonrası syntax doğrulaması yapılır. |
| [prettyplease][s17] | `syn` tree'sini generated code için okunabilir Rust'a çevirir. | Çıktı biçimi makineden bağımsızlaştırılır. |
| [SSIM reference][s18] | SSIM yapısal benzerlik ölçüsüdür; çözünürlüğe göre downsample önerisi vardır. | Tek başına piksel farkı yerine geometri + SSIM kullanılır. |

Eski `/docs/rest-api/file-nodes-endpoints/` ve `/docs/rest-api/file-properties/`
adresleri araştırma sırasında 404 verdi. Güncel endpoint/node/property adresleri yukarıdaki
[s2], [s3] ve [s4] olarak kullanıldı.

## Kanonik Girdi

### Snapshot sözleşmesi

Her derleme aşağıdaki metadata'yı taşıyan bir snapshot ile başlar:

```text
Snapshot {
    source_kind: Rest | Plugin | Fixture,
    file_key: optional string,
    file_version: optional string,
    root_node_ids: ordered list,
    raw_document: source payload,
    assets: downloaded bytes + content hash,
}
```

REST tarafında `version` parametresi ve seçili `ids`/`depth` kullanılır. Görsel referans
olarak kullanılan Figma image URL'leri fixture üretiminde hemen indirilir; URL'yi kalıcı
asset yolu olarak saklamak güvenilir değildir. Her asset'in kaynak URL'si, node ID'si,
formatı, scale'i ve yerel içerik hash'i manifestte tutulur.

Plugin tarafında dinamik sayfa yükleme ve asenkron API çağrıları snapshot alınmadan önce
tamamlanır. Compiler çalışırken Figma editor state'ine erişmeye çalışmaz.

### Determinizm kuralları

Aynı snapshot ve aynı compiler sürümü aynı IR bytes'ını ve aynı Rust source bytes'ını
üretmelidir. Bunun için:

- Kaynak node ID'si her zaman korunur; traversal sırasından ordinal ID üretilmez.
- IR node ID'si `file_key + source_node_id` ile nitelendirilir. File key yoksa snapshot
  scope'u ve source node ID birlikte kullanılır.
- Kaynakta ID bulunmuyorsa yalnızca açıkça belirtilmiş canonical path ve source child
  sırası fallback olarak kullanılır; bu durum diagnostic'e yazılır.
- Node children sırası, Figma'nın anlamlı çizim/z-order sırasıysa korunur. Map alanları
  anahtara göre sıralanır; component, variant, prop, import ve asset listeleri canonical
  anahtara göre sıralanır.
- `HashMap` iteration order, Rust `DefaultHasher`, random UUID, timestamp ve host path
  generated output'u etkileyemez.
- `-0.0`, NaN ve sonsuz değerler normalize edilir; geçersiz floating-point değerleri
  sessizce çevrilmez, diagnostic üretir.
- Fiziksel ölçüler canonical decimal formda, sabit hassasiyetle saklanır. Başlangıç
  toleransı `1e-4` logical px'tir; bu görsel kabul eşiği değildir.
- Renk kanalları `0..1` aralığında açık RGBA olarak tutulur. Premultiplication ve renk
  uzayı dönüşümü render/visual katmanına bırakılır.
- Display name ile Rust symbol name ayrıdır. Rust keyword, boşluk, Unicode ve collision
  dönüşümü tek bir versioned `NamePolicy` ile yapılır.
- Aynı isim çakıştığında suffix, sorted source ID'den türetilir; node listesi içindeki
  pozisyondan türetilmez.
- Canonical bytes için versioned bir encoder kullanılır ve fingerprint manifestte
  saklanır. Hash algoritması açıkça seçilir; platformun varsayılan hash'i kullanılmaz.

Canonicalization çıktısı yalnızca normalize edilmiş değerleri içermez; her kaynağın
`source_path`, `source_node_id` ve mümkünse `source_property` bilgisini de taşır. Böylece
bir hata generated Rust satırından tekrar Figma node/property'sine izlenebilir.

## Design IR

IR, Figma REST şemasının kopyası değildir. Figma'da bulunan ama hedef runtime'da anlamı
olmayan veya henüz desteklenmeyen alanlar `Unsupported`/diagnostic olarak korunur.

Önerilen ana şekil:

```rust
pub struct DesignDocument {
    pub schema_version: u16,
    pub source: SourceRef,
    pub roots: Vec<NodeId>,
    pub nodes: Vec<Node>,
    pub components: Vec<ComponentDef>,
    pub tokens: Vec<TokenDef>,
    pub assets: Vec<AssetRef>,
    pub diagnostics: Vec<Diagnostic>,
}

pub struct Node {
    pub id: NodeId,
    pub source: SourceNodeRef,
    pub parent: Option<NodeId>,
    pub children: Vec<NodeId>,
    pub kind: NodeKind,
    pub layout: LayoutSpec,
    pub paint: PaintSpec,
    pub content: ContentSpec,
    pub component_ref: Option<ComponentRef>,
}

pub enum NodeKind {
    Container,
    Text,
    Shape,
    Image,
    Instance,
    Slot,
    Unsupported { source_type: String },
}
```

### Layout ve geometri

`LayoutSpec` aşağıdaki kavramları Figma alanlarından ayırır:

- `layout_mode`: none, horizontal, vertical veya grid;
- primary/counter axis sizing: fixed, hug, fill;
- padding, item/counter-axis gap ve alignment;
- absolute/manual positioning ve clipping;
- local transform ve source absolute bounds;
- stroke'un layout'a dahil olup olmadığı;
- minimum/maksimum ölçüler;
- child order ve z-order.

GPUI lowering politikası:

| Figma kavramı | GPUI lowering yaklaşımı |
|---|---|
| Horizontal/vertical auto-layout | Flex yönü, gap, padding ve alignment builder'ları |
| `FILL` | Parent constraint içinde büyüyen child; sabit width/height ile taklit edilmez |
| `HUG` | İçeriğin doğal ölçüsü; text/image ölçüm koşulları açıkça raporlanır |
| Grid | Hedef GPUI primitive'i varsa grid; yoksa sınırlı ve açık fallback |
| `ABSOLUTE` child | Absolute element veya özel geometry backend'i |
| Rotation/shear/vector path | Canvas/path/asset backend'i; sıradan container'a zorlanmaz |
| `clipsContent` | Clip/overflow semantic'i |
| Unsupported layout | IR'da korunur, warning veya strict modda error |

`absoluteBoundingBox`, `relativeTransform` ve `absoluteRenderBounds` birbirinin yerine
kullanılmaz. İlki hizalama kanıtı, ikincisi parent-relative transform, üçüncüsü ise
stroke/effect taşmalarını kontrol etmek için kullanılır.

### Paint, text ve asset

Paint IR; solid, gradient, image, opacity, stroke ve effect'i ayrı tutar. Bir paint'in
görselde görünür olmaması ile alanın kaynağa hiç eklenmemiş olması farklı durumlar olarak
korunur.

Text IR en azından karakterler, font family/PostScript adı, weight/style, size, line
height, letter spacing, alignment, case, decoration, truncation ve fills alanlarını
içerir. Font bulunamadığında fallback font seçip geçmek yerine fixture ve diagnostic
üretimi yapılır; aksi halde görsel regresyon sonucu yanlış yorumlanır.

Image asset'leri içerik hash'iyle adreslenir. Geçici Figma download URL'si generated source'a
yazılmaz. Asset yoksa generated code kırık bir URL üretmez; `MissingAsset` diagnostic'i
ve görünür placeholder politikası uygulanır.

## Component, Variant ve Prop Mapping

```rust
pub struct ComponentDef {
    pub id: ComponentId,
    pub source: SourceNodeRef,
    pub display_name: String,
    pub root: NodeId,
    pub properties: Vec<PropertyDef>,
    pub variants: Vec<VariantDef>,
    pub slots: Vec<SlotDef>,
    pub code_mapping: Option<CodeMapping>,
}

pub enum PropertyKind {
    Boolean,
    Text,
    Variant { options: Vec<String> },
    InstanceSwap,
    Slot,
}

pub struct PropertyDef {
    pub id: PropertyId,
    pub source_name: String,
    pub rust_name: String,
    pub kind: PropertyKind,
    pub default_value: Scalar,
}
```

Figma component property türleri `BOOLEAN`, `INSTANCE_SWAP`, `TEXT` ve `VARIANT` olarak
IR'da typed property'lere çevrilir. Mapping tablosu:

| Figma | IR | GPUI varsayılanı |
|---|---|---|
| `BOOLEAN` | `PropertyKind::Boolean` | `bool` |
| `TEXT` | `PropertyKind::Text` | `SharedString` veya açıkça stateful ise `String` |
| `VARIANT` | sınırlı seçenekli enum metadata | generated enum/variant policy |
| `INSTANCE_SWAP` | component reference/factory | mapped component veya explicit fallback |
| slot property | named `SlotDef` | named child/render slot |

Variant için bilinmeyen değer sessizce default'a düşürülmez. Strict codegen error veya
`Unknown(String)` fallback'i hedef backend politikasına göre seçilir; seçim IR seviyesinde
gizlenmez.

Mapping önceliği:

1. Açık Code Connect mapping.
2. Figma component key/node reference ile kayıtlı proje mapping'i.
3. Aynı source scope içindeki canonical component name.
4. Structural component üretimi.

Birden fazla eşleşme aynı öncelikteyse compiler tahmin etmez; source path ve adayları
gösteren diagnostic verir. Component property mapping ile layout inheritance ayrı tutulur:
bir prop'un code adını bilmek, component'in görsel geometrisini otomatik olarak değiştirmez.

## Code Connect Kararı

### Karar: Opsiyonel adapter, core bağımlılığı değil

Code Connect'in resmi amacı Figma Dev Mode'da gerçek code component snippet'lerini ve
property mapping'lerini göstermektir. Template API; `getBoolean`, `getString`, `getEnum`,
`getSlot`, nested instance ve `executeTemplate` gibi erişimler sağlar. Bu, aşağıdaki
metadata'yı üretmek için değerlidir:

```text
CodeMapping {
    figma_document_url,
    figma_node_id,
    source_path,
    code_symbol,
    property_map: figma_property -> code_property,
    slot_map,
    nested_component_map,
}
```

Ancak Code Connect:

- Figma node geometrisinin yerine geçmez;
- compiler'ın canonical IR'ını tanımlamaz;
- GPUI runtime davranışını otomatik olarak çözmez;
- template içindeki keyfi TypeScript'i core içinde çalıştırmamalıdır;
- Organization/Enterprise planı ve uygun seat gerektirir.

Yeni entegrasyon template dosyalarını hedefler. Legacy framework parser'larına yeni
bağlanmak, parser'ların artık aktif bakım almaması nedeniyle yanlış sınırdır. Adapter
template'leri statik mapping manifestine çevirir; template execution gerekiyorsa bu,
compiler core dışında yayınlama/Dev Mode aşamasında kalır.

| Seçenek | Artı | Eksi | Karar |
|---|---|---|---|
| Code Connect'i core'a bağlamak | Hazır mapping ve Dev Mode context'i | Plan, CLI, template runtime ve format değişikliği core'u kirletir | Reddedildi |
| Code Connect adapter'ı | Gerçek component/property/slot mapping'i taşır | Ayrı CLI/seat ve metadata yaşam döngüsü gerekir | Seçildi |
| Sadece Figma structural data | Dış servis ve lisans bağı yok | Gerçek code component mapping'i eksik kalır | Fallback olarak desteklenir |

## GPUI Rust Codegen

### Aşamalar

```text
ResolvedComponent
    -> backend semantic model
    -> GPUI element/component plan
    -> proc_macro2 TokenStream
    -> syn::File parse + structural validation
    -> prettyplease::unparse
    -> generated .rs + manifest
```

`quote` ile token üretimi, `syn` ile parse/validation ve `prettyplease` ile generated code
formatlanması seçilir. Bu üçlü core IR'a değil, yalnızca `codegen-gpui` crate'ine bağlıdır.
`proc-macro2` kullanılması generator'ın procedural macro olmadan normal binary/test
bağlamında çalışmasını sağlar.

Codegen kuralları:

- Generated modüller component ID/name sırasına göre yazılır.
- Import'lar canonical path'e göre deduplicate ve sort edilir.
- `use`, identifier ve literal üretimi token/AST üzerinden yapılır; ham source string
  birleştirme yalnızca kontrollü literal içerikleri için kullanılır.
- `syn::parse2` başarısızsa çıktı dosyası yazılmaz.
- Generated header timestamp içermez; schema version, input fingerprint ve generator
  version içerir.
- `rustfmt` son kullanıcı tercihidir; compiler'ın deterministic ilk formatlayıcısı
  `prettyplease` olur. CI'da `cargo fmt --check` ek doğrulama olabilir.
- Aynı component'in state gerektirmeyen görünümü için hedef GPUI sürümünün
  `RenderOnce`/`IntoElement` sözleşmesi; gözlem, focus, async veya retained state isteyen
  görünümü için `Render`/`Entity` sözleşmesi kullanılır.
- Her repeated element için kaynak node ID'den türetilen stable `ElementId` kullanılır;
  random ID veya mutable list index'i identity olarak kullanılmaz.
- `render` içinde network, dosya yazma, mutation loop veya koşulsuz notify üretilmez.
- GPUI Component kullanılıyorsa initialization bir kez yapılır ve window-level Root/
  overlay/focus ownership bypass edilmez.
- Figma static color/spacing değeri önce design token'a bağlanır. Token yoksa literal,
  generated token/foundation dosyasında tutulur; ürün component renderer'larına dağılmaz.

GPUI API sürümü bu boş çalışma alanında henüz pinlenmemiştir. Güncel GPUI dokümantasyonunda
boot için `gpui_platform::application()` ve stateful view için entity/context ayrımı
izlenmektedir; implementation başlamadan önce hedef Zed/GPUI commit'i Cargo manifestinde
pinlenmelidir. Bu nedenle bu belge belirli builder method imzasını core sözleşmesi yapmaz.

### Unsupported ve diagnostics

Her node için codegen sonucu `Generated`, `GeneratedWithWarning` veya `Rejected` olabilir.
Örneğin blend mode, progressive blur, variable stroke, prototype transition veya özel
vector path hedef backend tarafından desteklenmiyorsa:

```text
E-GPUI-UNSUPPORTED
  source: file=<...> node=<...> property=<...>
  feature: <feature>
  policy: strict | placeholder | custom-backend
```

Sessizce `div()` ile yerine koymak görsel doğruluğu bozduğu için default policy strict
fixture derlemesinde hata, exploratory modda warning'dir.

## Visual Regression

### Fixture edinme

Önerilen dizinler:

```text
fixtures/
  figma/raw/          # Figma JSON snapshot
  figma/canonical/    # canonical input bytes
  ir/                 # checked-in Design IR snapshots
  assets/             # content-hash adresli images/fonts
  reference/          # downloaded Figma PNG/SVG references
  gpui/               # target render manifests
  reports/            # diff, heatmap and diagnostic outputs
```

Her fixture manifesti en az şunları içerir:

```text
fixture_id, source_file_key, source_version, root_node_id,
viewport, scale_factor, locale, theme, font_set,
figma_reference_sha256, canonical_ir_sha256,
gpui_commit, generator_version
```

Figma image endpoint'i node render'ı ve `scale`/`format` seçeneklerini sağlar. Referans
görüntü indirildikten sonra URL değil hash'lenmiş dosya kullanılmalıdır. Figma SVG text'i
outline veya text olarak dışa aktarabilir; text seçimi rasterizer farklarını etkilediği
için fixture manifestinde açıkça belirtilir.

### Karşılaştırma sırası

1. **IR/hiyerarşi:** node count, parent-child ilişkisi, child order ve component binding
   aynı mı?
2. **Geometri:** logical bounds, clip, overflow, baseline ve önemli anchor'lar aynı mı?
3. **Piksel:** aynı viewport/scale/font/rasterizer koşulunda kanal farkı.
4. **Yapısal görsel:** luminance SSIM ve edge/low-frequency farkı.
5. **Triage:** diff heatmap, source node ID ve generated Rust span'ı birlikte raporlanır.

Geometri veya hiyerarşi hatası SSIM yüksek olsa bile pass sayılmaz. SSIM yüksekliği yanlış
node order, yanlış click target veya görünmeyen clipping hatasını saklayabilir.

### Başlangıç eşikleri

Aşağıdaki değerler SSIM literatürünün evrensel kabul sınırları değildir; sabit bir GPUI
render ortamı kurulduktan sonra 20+ fixture ile kalibre edilecek proje başlangıç
politikasıdır.

| Ölçüm | Başlangıç politikası | Sonuç |
|---|---:|---|
| Canonical IR bytes | Tam eşitlik | Hard fail |
| Hiyerarşi/child order | Tam eşitlik | Hard fail |
| Normalize float | `1e-4` logical px | Canonicalization tolerance |
| Node geometry | `max abs <= 0.5` logical px ve en fazla 1 physical px rounding | Hard fail |
| Kanal farkı | `max <= 2/255`; bad pixel ratio `<= 0.1%` | Aynı rasterizer'da hard fail |
| Luminance SSIM | `>= 0.995` | Hard fail, eşik kalibre edilir |
| Edge/low-frequency diff | Heatmap ve p95 raporu | İlk aşamada triage; kalibrasyon sonrası gate |

Pixel-only farkı bilinen font antialiasing veya platform rasterizer drift'inden kaynaklanıp
IR/geometri/SSIM geçiyorsa warning olabilir. Bu istisna fixture manifestinde platform,
font ve karar nedeni ile kaydedilir; genel bir "visual diff ignore" listesi oluşturulmaz.

SSIM uygulaması için referans sayfadaki çözünürlük ölçekleme önerisi dikkate alınır:
görüntü boyutuna göre average/downsample sonrası luminance karşılaştırması yapılır.
Renk/alpha karşılaştırması ayrıca yapılır; SSIM'in tek kanala indirgenmesi renk hatasını
gizlememelidir.

### Geometri kanıtı

Render harness, mümkün olan node'larda IR node ID'yi GPUI element identity veya debug
metadata ile eşler. Her fixture için:

- expected/source bounds;
- actual resolved bounds;
- clip/overflow sonucu;
- text baseline ve line count;
- missing asset/font;
- source node -> generated symbol mapping

manifestte saklanır. Node ID render'a taşınamıyorsa parent path ve stable generated symbol
ile fallback yapılır; bu fallback reliability seviyesini düşürür ve raporda görünür.

## Üçüncü Taraflardan Alınan ve Alınmayanlar

| Proje | Alınan fikir | Alınmayan şey | Lisans durumu |
|---|---|---|---|
| Mitosis | Abstract node, binding, children/slots ve control-flow ayrımı | Mitosis node tiplerini doğrudan bağımlılık yapmak | MIT |
| TeleportHQ | UIDL ve validation/parse/resolve/generator düşüncesi | Web/React odaklı runtime varsayımları | MIT |
| FigmaToCode | Selection traversal, limit, warning ve fail-fast yaklaşımı | Kaynak kod veya GPL implementation | GPL-3.0; core'a kopyalanmaz |
| Figma SDS | Token, styles, Code Connect ve file-agnostic URL mapping örnekleri | React/SDS component API'si | MIT |
| Code Connect | Component/property/slot mapping ve Dev Mode context'i | Core IR, GPUI runtime veya keyfi template execution | Repository MIT; servis plan/seat kısıtları ayrı |
| Anima/Locofy/kapalı Builder | Yalnız kamuya açık capability iddiaları | İç pipeline, heuristic veya proprietary algorithm iddiası | Doğrulanmadı; kullanıma alınmadı |

## Lisans ve Bağımlılık Politikası

- `core-ir`, normalization ve resolution kodu üçüncü taraf runtime crate'i gerektirmemeli.
- `proc-macro2`, `quote`, `syn` ve `prettyplease` codegen sınırında tutulmalı; araştırma
  tarihindeki docs.rs metadata'sında hepsi `MIT OR Apache-2.0` olarak görünüyor.
- Sürüm aralıkları değil, workspace lockfile ile kesin sürümler pinlenmeli. Araştırma
  snapshot'ında görülen sürümler sırasıyla `proc-macro2 1.0.107`, `quote 1.0.47`,
  `syn 3.0.4`, `prettyplease 0.3.0` idi; implementasyon sırasında yeniden doğrulanır.
- FigmaToCode GPL-3.0 kaynak kodu, test fixture'ı dışında core'a kopyalanmaz. Sadece
  davranış fikri ve kamuya açık algoritmik gözlem kullanılabilir.
- MIT kaynaklardan kod kopyalanırsa copyright/license notice korunur; tercih edilen yol
  kopyalamak yerine davranışı bağımsız yeniden uygulamaktır.
- Her yeni dependency için `LICENSE`, transitive license ve binary distribution etkisi
  `LICENSE-LEDGER.md` içinde kaydedilmelidir.
- Figma token, URL veya fixture içinde access token/secret tutulmaz.

## Önerilen Çalışma Alanı

Bu dizin şu anda boş olduğu için aşağıdaki yapı hedeftir; henüz oluşturulmuş crate değildir:

```text
crates/
  design-ir/
  canonicalize/
  resolve/
  input-figma/
  input-code-connect/
  codegen-gpui/
  visual-fixtures/
  figma-rust-cli/
fixtures/
docs/
LICENSE-LEDGER.md
```

Crate bağımlılık yönü tek yönlü olmalıdır:

```text
input-* -> canonicalize -> design-ir -> resolve -> codegen-gpui
                                              \-> visual-fixtures
```

`design-ir` hiçbir üst katmana veya GPUI crate'ine bağlanmaz. `input-code-connect`,
`design-ir` içine yalnız normalize edilmiş `CodeMapping` gönderir; Code Connect template
runtime'ı IR crate'ine taşınmaz.

## Geçiş Planı ve Kabul Kriterleri

1. **Bootstrap:** Cargo workspace, schema version, fixture manifest ve license ledger.
   Kabul: boş fixture için deterministic CLI çıktısı ve clean dependency graph.
2. **IR/normalizer:** Figma node/property fixture'ları, canonical bytes ve diagnostics.
   Kabul: aynı fixture iki çalıştırmada byte-identical IR; map/order testleri.
3. **Static GPUI backend:** container, shape, text ve image için minimal lowering.
   Kabul: generated `.rs` `syn` parse eder, compile olur, source mapping manifesti oluşur.
4. **Component resolver:** component set, variant, boolean/text/instance-swap/slot
   mapping'i.
   Kabul: explicit mapping önceliği, ambiguity error ve unknown variant testleri.
5. **Code Connect adapter:** template metadata'sından statik mapping manifesti.
   Kabul: Code Connect olmadan structural compile çalışmaya devam eder; adapter yalnız
   mapping ekler.
6. **Render harness:** sabit viewport/font/theme/scale ile GPUI capture ve geometry
   manifesti.
   Kabul: missing font/asset ve unsupported feature fail-closed raporlanır.
7. **Visual gate:** pixel, SSIM, edge/low-frequency diff ve heatmap.
   Kabul: eşikler fixture noise baseline ile kalibre edilir; platform drift'i açıkça
   manifestte tutulur.

## Riskler ve Açık Kararlar

- Hedef GPUI ve varsa `gpui-component` commit'i henüz belirlenmedi. Codegen API'si bu pin
  yapılmadan implementation contract olarak sabitlenmemeli.
- Figma auto-layout ile GPUI layout semantics bire bir değildir; `FILL`, `HUG`, grid,
  text wrapping ve fractional pixel için gerçek fixture gerekir.
- Font ve rasterizer farkları pixel gate'i bozabilir. Bu yüzden geometri/hiyerarşi hard
  gate'leri görsel metriklerden önce çalışır.
- Figma prototype interactions, variables/modes ve bazı effects structural görünümden
  daha geniş runtime semantics ister; ilk sürümde diagnostic/custom adapter'dır.
- Figma API image URL'leri geçicidir; fixture capture sırasında indirme başarısızlığı
  derlemeyi geçirmemelidir.
- Code Connect plan/seat ve CLI yaşam döngüsü core compiler'ın offline kullanımını
  engellememelidir.
- JSON parsing ve PNG codec seçimi implementation bootstrap'ında ayrıca lisans ve
  deterministic serialization açısından kararlaştırılmalıdır; bu karar core IR'a
  dependency eklememelidir.

## ADR Özeti

### ADR-001: Core IR bağımsızlığı

Figma REST şeması ve GPUI API'si domain IR'ın public contract'ı yapılmaz. Gerekçe:
Figma schema değişimleri ile GPUI backend değişimleri birbirinden bağımsız evrilebilsin.

### ADR-002: Code Connect adapter sınırı

Code Connect mapping/property/slot context sağlar; structural input, canonicalization ve
codegen onsuz çalışır. Gerekçe: Code Connect'in amacı Dev Mode code context'i, ayrıca
template/seat/CLI yaşam döngüsü core compiler için zorunlu değildir.

### ADR-003: AST tabanlı Rust üretimi

Token üret, parse et, formatla; source string concat ile Rust yazma. Gerekçe: syntax
güvenliği, deterministic imports ve test edilebilir generated output.

### ADR-004: İki bağımsız doğrulama yüzeyi

IR/geometri kanıtı ile raster görsel metrikleri birlikte tutulur. Gerekçe: SSIM veya pixel
benzerliği tek başına yanlış hierarchy, clipping, stable identity veya hit-target hatasını
kanıtlamaz.

## Kaynaklar

- [s1] https://developers.figma.com/docs/rest-api/
- [s2] https://developers.figma.com/docs/rest-api/file-endpoints/
- [s3] https://developers.figma.com/docs/rest-api/file-node-types/
- [s4] https://developers.figma.com/docs/rest-api/file-property-types/
- [s5] https://www.figma.com/plugin-docs/
- [s6] https://developers.figma.com/docs/code-connect/
- [s7] https://developers.figma.com/docs/code-connect/template-files/
- [s8] https://developers.figma.com/docs/code-connect/templates-migration-guide/
- [s9] https://github.com/BuilderIO/mitosis/blob/main/packages/core/src/types/mitosis-node.ts
- [s10] https://github.com/teleporthq/teleport-code-generators/blob/development/packages/teleport-types/src/uidl.ts
- [s11] https://github.com/bernaferrari/FigmaToCode/blob/main/packages/backend/src/code.ts
- [s12] https://github.com/bernaferrari/FigmaToCode/blob/main/LICENSE
- [s13] https://github.com/figma/sds
- [s14] https://docs.rs/proc-macro2/latest/proc_macro2/
- [s15] https://docs.rs/quote/latest/quote/
- [s16] https://docs.rs/syn/latest/syn/
- [s17] https://docs.rs/prettyplease/latest/prettyplease/
- [s18] https://ece.uwaterloo.ca/~z70wang/research/ssim/
- [s19] https://github.com/zed-industries/zed/tree/main/crates/gpui
- [s20] https://github.com/longbridge/gpui-component
