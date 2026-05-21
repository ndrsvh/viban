/** Mirror of `viban_core::types::Session`. */
export interface Session {
  id: string;
  claude_session_id: string | null;
  title: string;
  created_at: number;
  project_path: string;
}

/** Mirror of `viban_core::types::Message`. */
export interface Message {
  id: string;
  session_id: string;
  role: string;
  content: string;
  created_at: number;
  raw_json: string | null;
}
