# Q0005: Should CI be a required check on `main`?

- **Asked:** 2026-11-02
- **Blocking:** no
- **Default until answered:** not required. Agents read CI results after pushing and fix failures.

## Question

The remediation plan's Phase 0 exit asks for CI to be required on the default branch. All three earlier CI runs on
`main` failed, and the jobs added since have never run. Agents now push directly to `main`, so a required check
would need either a pull-request workflow or a bypass for this account.

The recommendation is to decide this after CI has been green for a while.

## Answer

Owner (2026-11-03): CI is not required on the main branch. We should regularly take a look and fix if we notice that it is red. But it is no longer a requirement!
