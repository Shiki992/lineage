# Security policy

## Reporting a vulnerability

Please **do not** open a public issue for security problems.

Report them privately through GitHub instead:

1. Go to the repository's **Security** tab.
2. Choose **Report a vulnerability**.

Only the maintainer can see the report. The maintainer aims to reply within 7 days. We'll agree a fix and a disclosure date with you, and credit you unless you'd rather stay anonymous.

If the button isn't there, open an issue that says only "please enable private vulnerability reporting", with no details.

## What counts

Lineage reads documents that come back from other people, so **every returned file is untrusted input**. We especially want to hear about:

- a crafted .docx, .pptx or .xlsx that crashes the engine, hangs it, uses unbounded memory, or makes it write a damaged file,
- anything that leaks history, comments, tracked changes or hidden data into a "clean" sent copy,
- anything that sends a user's documents or history anywhere other than their own storage,
- storage permissions broader than documented.

## Supported versions

Lineage has no stable release yet. Fixes go to the `main` branch.
