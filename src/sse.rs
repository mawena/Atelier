//! Lecture incrémentale d'un flux Server-Sent Events.

/// Accumule des octets et rend les champs `data:` des événements complets.
#[derive(Default)]
pub struct SseParser {
    buf: String,
    data: Vec<String>,
}

impl SseParser {
    pub fn push(&mut self, chunk: &[u8]) -> Vec<String> {
        self.buf.push_str(&String::from_utf8_lossy(chunk));
        let mut events = Vec::new();
        while let Some(pos) = self.buf.find('\n') {
            let line: String = self.buf.drain(..=pos).collect();
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    events.push(self.data.join("\n"));
                    self.data.clear();
                }
            } else if let Some(rest) = line.strip_prefix("data:") {
                self.data
                    .push(rest.strip_prefix(' ').unwrap_or(rest).to_string());
            }
            // Les autres champs (event:, id:, commentaires) sont ignorés.
        }
        events
    }

    /// Événement éventuellement resté en attente à la fin du flux.
    pub fn finish(&mut self) -> Option<String> {
        if !self.buf.trim().is_empty() {
            let rest = std::mem::take(&mut self.buf);
            if let Some(d) = rest.trim().strip_prefix("data:") {
                self.data.push(d.trim_start().to_string());
            }
        }
        (!self.data.is_empty()).then(|| std::mem::take(&mut self.data).join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_across_chunks() {
        let mut p = SseParser::default();
        assert!(p.push(b"data: {\"a\":").is_empty());
        let ev = p.push(b"1}\n\ndata: [DONE]\n\n");
        assert_eq!(ev, vec!["{\"a\":1}", "[DONE]"]);
    }

    #[test]
    fn crlf_and_comments() {
        let mut p = SseParser::default();
        let ev = p.push(b": keep-alive\r\nevent: message\r\ndata: x\r\n\r\n");
        assert_eq!(ev, vec!["x"]);
    }

    #[test]
    fn multi_line_data_and_finish() {
        let mut p = SseParser::default();
        assert_eq!(p.push(b"data: a\ndata: b\n\n"), vec!["a\nb"]);
        assert!(p.push(b"data: fin").is_empty());
        assert_eq!(p.finish().as_deref(), Some("fin"));
    }
}
