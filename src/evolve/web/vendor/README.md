# Vendored browser dependencies

These assets are served from the loopback self-evolve origin so third-party
scripts never receive the IDE session token.

| Package | Version | Entry SHA-256 |
| --- | --- | --- |
| d3 | 7.9.0 | `f2094bbf6141b359722c4fe454eb6c4b0f0e42cc10cc7af921fc158fceb86539` |
| lucide | 0.468.0 | `3411692820cb8d47543f69496aa25fd603a358f4498046f41c508a5a3342210e` |
| monaco-editor | 0.44.0 | `fa0ef7ea8ead3713f5b968036b818d30bbb4715c39ae977d8affc9905aa32588` |

The package license text is stored beside each package. Replace an entire
package directory from an exact npm version when upgrading, update the hashes,
then rerun the browser CSP and visual checks.
