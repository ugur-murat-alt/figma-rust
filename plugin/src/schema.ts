export const SCHEMA_VERSION = 2;
export const PLUGIN_TYPINGS_VERSION = "1.135.0";
export const EXTRACTOR_NAME = "figma-rust-plugin";
export const EXTRACTOR_VERSION = "0.4.0";

export type JsonValue =
  | null
  | boolean
  | number
  | string
  | JsonValue[]
  | { [key: string]: JsonValue };

export type ExtractionSeverity = "INFO" | "WARNING" | "ERROR";

export interface ExtractionDiagnostic {
  severity: ExtractionSeverity;
  code: string;
  message: string;
  node_id?: string;
  property_path?: string;
  help?: string;
}

export type RawExtensions = Record<string, JsonValue>;

export interface RawSource {
  extractor?: string;
  extractor_version?: string;
  file_key?: string;
  page_id: string;
  selected_node_ids: string[];
  plugin_api_version: string;
}

export type RawNodeKind =
  | "FRAME"
  | "GROUP"
  | "RECTANGLE"
  | "ELLIPSE"
  | "TEXT"
  | "VECTOR"
  | "IMAGE"
  | "COMPONENT"
  | "COMPONENT_SET"
  | "INSTANCE"
  | "SLOT"
  | "SLICE"
  | "SCROLL";

export type RawLayoutMode = "NONE" | "HORIZONTAL" | "VERTICAL" | "GRID";
export type RawAlignment = "START" | "CENTER" | "END" | "SPACE_BETWEEN" | "BASELINE" | "STRETCH";
export type RawChildAlignment = "INHERIT" | "MIN" | "CENTER" | "MAX" | "STRETCH";
export type RawAxisSizing = "HUG" | "FILL" | "FIXED";
export type RawPositioning = "AUTO" | "ABSOLUTE";
export type RawConstraint = "MIN" | "CENTER" | "MAX" | "STRETCH" | "SCALE";
export type RawStrokeAlign = "INSIDE" | "CENTER" | "OUTSIDE";
export type RawBlendMode = "NORMAL" | "PASS_THROUGH" | "MULTIPLY" | "SCREEN" | "OVERLAY" | "OTHER";

export type RawGridTrack =
  | { kind: "FIXED"; value: number }
  | { kind: "FLEX"; value: number }
  | { kind: "HUG" };

export interface RawGridPlacement {
  row: number;
  column: number;
  row_span?: number;
  column_span?: number;
}

export interface RawGrid {
  columns: RawGridTrack[];
  rows: RawGridTrack[];
  column_gap: number;
  row_gap: number;
}

export interface RawScroll {
  horizontal: boolean;
  vertical: boolean;
}

export interface RawLayout {
  mode: RawLayoutMode;
  wrap: boolean;
  primary_alignment: RawAlignment;
  counter_alignment: RawAlignment;
  child_counter_alignment: RawChildAlignment;
  gap: RawBoundValue<number>;
  padding: RawBoundEdges;
  grid: RawGrid;
  clips_content: boolean;
  scroll: RawScroll;
}

export interface RawSize {
  width?: RawBoundValue<number>;
  height?: RawBoundValue<number>;
  horizontal?: RawAxisSizing;
  vertical?: RawAxisSizing;
  min_width?: RawBoundValue<number>;
  max_width?: RawBoundValue<number>;
  min_height?: RawBoundValue<number>;
  max_height?: RawBoundValue<number>;
  aspect_ratio?: number;
}

export interface RawTransform {
  matrix: [number, number, number, number, number, number];
}

export interface RawPosition {
  positioning: RawPositioning;
  x: number;
  y: number;
  horizontal_constraint: RawConstraint;
  vertical_constraint: RawConstraint;
  grid?: RawGridPlacement;
  transform: RawTransform;
}

export interface RawColor {
  r: number;
  g: number;
  b: number;
  a: number;
}

export interface RawBoundValue<T> {
  literal: T;
  token_id?: string;
  mode_context?: RawModeContext;
}

export type RawModeContext = Record<string, string>;

export interface RawBoundEdges {
  top: RawBoundValue<number>;
  right: RawBoundValue<number>;
  bottom: RawBoundValue<number>;
  left: RawBoundValue<number>;
}

export type RawGradientKind = "LINEAR" | "RADIAL" | "ANGULAR" | "DIAMOND";
export type RawImageScaleMode = "FIT" | "FILL" | "CROP" | "TILE";

export interface RawGradientStop {
  position: number;
  color: RawBoundValue<RawColor>;
}

export type RawPaint =
  | { kind: "SOLID"; color: RawBoundValue<RawColor> }
  | { kind: "GRADIENT"; gradient_kind: RawGradientKind; stops: RawGradientStop[] }
  | {
      kind: "IMAGE";
      asset_id: string;
      scale_mode: RawImageScaleMode;
      image_transform?: RawTransform;
      opacity: number;
      rotation?: number;
      has_filters: boolean;
    }
  | { kind: "VIDEO" }
  | { kind: "PATTERN" }
  | { kind: "SHADER" };

export type RawEffect =
  | {
      kind: "DROP_SHADOW";
      color: RawBoundValue<RawColor>;
      offset_x: number;
      offset_y: number;
      blur: number;
      spread: number;
    }
  | {
      kind: "INNER_SHADOW";
      color: RawBoundValue<RawColor>;
      offset_x: number;
      offset_y: number;
      blur: number;
      spread: number;
    }
  | { kind: "LAYER_BLUR"; radius: number }
  | { kind: "BACKGROUND_BLUR"; radius: number }
  | { kind: "PROGRESSIVE_BLUR" }
  | { kind: "NOISE" }
  | { kind: "TEXTURE" }
  | { kind: "GLASS" }
  | { kind: "SHADER" };

export interface RawRadii {
  top_left: RawBoundValue<number>;
  top_right: RawBoundValue<number>;
  bottom_right: RawBoundValue<number>;
  bottom_left: RawBoundValue<number>;
  smoothing: number;
}

export interface RawStyle {
  fills: RawPaint[];
  strokes: RawPaint[];
  stroke_widths: RawBoundEdges;
  stroke_align: RawStrokeAlign;
  radii: RawRadii;
  effects: RawEffect[];
  blend_mode: RawBlendMode;
  is_mask: boolean;
}

export interface RawText {
  characters: string;
  auto_resize?: RawTextAutoResize;
  horizontal_alignment?: RawTextHorizontalAlignment;
  vertical_alignment?: RawTextVerticalAlignment;
  truncation?: RawTextTruncation;
  max_lines?: number;
  runs: RawTextRun[];
}

export type RawTextAutoResize = "NONE" | "WIDTH_AND_HEIGHT" | "HEIGHT" | "TRUNCATE";
export type RawTextHorizontalAlignment = "LEFT" | "CENTER" | "RIGHT" | "JUSTIFIED";
export type RawTextVerticalAlignment = "TOP" | "CENTER" | "BOTTOM";
export type RawTextTruncation = "DISABLED" | "ENDING";

export interface RawTextRun {
  start_utf16: number;
  end_utf16: number;
  style: RawTextStyle;
}

export interface RawTextStyle {
  font_family?: string;
  font_style?: string;
  font_size?: RawBoundValue<number>;
  font_weight?: number;
  line_height?: number;
  letter_spacing?: number;
  color?: RawBoundValue<RawColor>;
}

export type RawComponentRole = "COMPONENT" | "COMPONENT_SET" | "INSTANCE";
export interface RawPreferredComponent {
  type: "COMPONENT" | "COMPONENT_SET";
  key: string;
}
export interface RawSlotProperty {
  description?: string;
  preferred_values: RawPreferredComponent[];
  stretch_child_on_insert?: boolean;
  display_empty_by_default?: boolean;
  min_children?: number;
  max_children?: number;
  allow_preferred_values_only?: boolean;
}
export type RawComponentValue =
  | { kind: "VARIANT"; value: string }
  | { kind: "TEXT"; value: string }
  | { kind: "BOUND_TEXT"; value: RawBoundValue<string> }
  | { kind: "BOOLEAN"; value: boolean }
  | { kind: "BOUND_BOOLEAN"; value: RawBoundValue<boolean> }
  | { kind: "INSTANCE_SWAP"; value: string }
  | { kind: "SLOT"; value: RawSlotProperty };

export interface RawComponentPropertyReferences {
  visible?: string;
  characters?: string;
  main_component?: string;
}

export interface RawOverride {
  node_id: string;
  fields: string[];
}

export interface RawComponentMetadata {
  role: RawComponentRole;
  component_key: string;
  component_set_key?: string;
  variants: Record<string, string>;
  properties: Record<string, RawComponentValue>;
  overrides: RawOverride[];
}

export type RawTrigger =
  | { kind: "CLICK" }
  | { kind: "HOVER" }
  | { kind: "PRESS" }
  | { kind: "KEY"; key: string };

export type RawAction =
  | { kind: "EMIT"; name: string }
  | { kind: "NAVIGATE"; destination_id: string }
  | { kind: "OPEN_OVERLAY"; destination_id: string }
  | { kind: "SMART_ANIMATE"; destination_id: string };

export interface RawReaction {
  trigger: RawTrigger;
  action: RawAction;
}

export type RawLiteral =
  | { kind: "NUMBER"; value: number }
  | { kind: "COLOR"; value: RawColor }
  | { kind: "STRING"; value: string }
  | { kind: "BOOLEAN"; value: boolean };

export interface RawVariable {
  id: string;
  name: string;
  collection_id: string;
  mode_id: string;
  mode_context: RawModeContext;
  source_node_id?: string;
  value: RawLiteral;
}

export interface RawComponent {
  key: string;
  name: string;
  set_key?: string;
  property_definitions: Record<string, string>;
}

export interface RawAsset {
  id: string;
  source_node_id: string;
  media_type: string;
  content_hash?: string;
  export_settings: Record<string, string>;
  payload_base64?: string;
}

export interface RawNode {
  id: string;
  name: string;
  figma_node_type?: string;
  figma_locked?: boolean;
  kind: RawNodeKind;
  visible: boolean;
  opacity: number;
  layout: RawLayout;
  size: RawSize;
  position: RawPosition;
  style: RawStyle;
  text?: RawText;
  component?: RawComponentMetadata;
  component_property_references?: RawComponentPropertyReferences;
  reactions: RawReaction[];
  children: RawNode[];
}

export interface ExtractionTraversalChunk {
  index: number;
  start_node_index: number;
  end_node_index: number;
  node_count: number;
  first_node_id: string;
  last_node_id: string;
}

export interface ExtractionTraversalRoot {
  id: string;
  node_count: number;
  complete: boolean;
}

export interface ExtractionManifest {
  capabilities: string[];
  traversal: {
    chunk_node_limit: number;
    node_count: number;
    complete: boolean;
    chunks: ExtractionTraversalChunk[];
    roots: ExtractionTraversalRoot[];
  };
}

export interface ExtractionBundle {
  schema_version: number;
  source: RawSource;
  roots: RawNode[];
  variables: RawVariable[];
  components: RawComponent[];
  assets: RawAsset[];
  extraction_diagnostics: ExtractionDiagnostic[];
  extraction_manifest?: ExtractionManifest;
  rest_snapshot?: JsonValue;
}

export interface CompilerResponse {
  code?: string;
  diagnostic_groups?: DiagnosticGroup[];
  diagnostics?: ExtractionDiagnostic[];
  error?: string;
}

export interface DiagnosticGroup {
  severity: ExtractionDiagnostic["severity"];
  code: string;
  property_path?: string;
  selected_root_id?: string;
  count: number;
  samples: DiagnosticSample[];
}

export interface DiagnosticSample {
  node_id?: string;
  message: string;
  help?: string;
}

export interface BridgeExportResponse {
  path: string;
  byte_length: number;
  sha256: string;
  complete: boolean;
  traversal_complete: boolean | null;
}
