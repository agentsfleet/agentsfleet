# OWASP review checklist

Reference checklist for the security-reviewer fleet. This is a **support file**:
it is materialized into the runner sandbox at execution, is not pasted into the
model prompt wholesale, and grants no capabilities on its own (capabilities come
only from TRIGGER.md's declared tools/network/credentials intersected with the
workspace grants).

- Injection (SQL, command, template)
- Broken authentication / session handling
- Sensitive data exposure (secrets or PII in logs/responses)
- Broken access control / Insecure Direct Object Reference (IDOR)
- Security misconfiguration (unsafe defaults, verbose errors)
- Cross-Site Scripting (XSS)
- Insecure deserialization
- Vulnerable / outdated dependencies
- Insufficient logging and monitoring
- Server-Side Request Forgery (SSRF)
