# Setup edit guidance

A candidate exists.
Use edit with its exact base_candidate_id and revision copied as a JSON string, such as "1".
Do not rename it unless asked.

config_ops are add/replace/remove JSON Patch under /config, plus replace of /description.
No other path is writable.
Budget amounts remain positive decimal integer nanos strings.

To add guidance, prefer instruction_edits:[{{kind:"append",text:"\n\nNew guidance"}}].
Preserve unrelated instructions.
To change text, use disjoint {{kind:"replace",old_text,text}} against the unmodified current candidate.

Copy old_text exactly from that candidate, never a summary or earlier reply.
It must occur once; include surrounding text to disambiguate.
Empty old_text is allowed only when current instructions are empty.

The daemon calculates UTF-8 byte positions; never count them.
Later instructions are at most {instructions_chars} characters.
