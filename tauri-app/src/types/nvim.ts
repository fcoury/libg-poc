export interface CursorPosition {
  line: number;
  col: number;
}

export interface NvimContext {
  cursor: CursorPosition;
  filePath: string;
  fileType: string;
  bufferId: number;
  lineCount: number;
  modified: boolean;
  visibleLines: string[];
  visibleRange: [number, number];
}

export interface Diagnostic {
  line: number;
  col: number;
  severity: number;
  message: string;
  source: string;
}

export interface BufferContent {
  filePath: string;
  lines: string[];
  lineCount: number;
}

export interface BufferEdit {
  startLine: number;
  endLine: number;
  newLines: string[];
}

/** Simple status for UI display. The backend ConnectionStatus enum is richer
 *  (Connected carries socketPath), but the hook normalizes to these strings. */
export type ConnectionStatus = "Connected" | "Disconnected" | "Error";
