# Firma di cedro.exe e degli installatori Windows

Regole pubbliche: [CODE_SIGNING_POLICY.md](../../CODE_SIGNING_POLICY.md). Il certificato è di SignPath Foundation
(gratis per i progetti open source); la firma la fa SignPath.io a partire dalle build di GitHub Actions.

Come funziona nel workflow `windows-arm64.yml` (job `app` per ARM64 e `x64`), solo quando si pubblica una release:

1. `cedro.exe` va a SignPath (configurazione `exe`, file [exe.xml](exe.xml)) e torna firmato in `target/release`;
2. `installer/build.ps1 -SoloInstallatore` rifà l'installatore con l'exe firmato, senza ricompilare;
3. l'installatore e la sua copia Bondeno vanno a SignPath (configurazione `installatore`, [installatore.xml](installatore.xml)).

Ogni richiesta resta in attesa finché un approvatore non la approva su SignPath (c'è un'ora di tempo).
Senza il segreto `SIGNPATH_API_TOKEN` i passi di firma si saltano e tutto va come prima.

Da impostare nel repo dopo l'accettazione (Settings → Secrets and variables → Actions):

| Tipo | Nome | Valore |
|---|---|---|
| segreto | `SIGNPATH_API_TOKEN` | token API dell'utente CI di SignPath (ruolo *Submitter*) |
| variabile | `SIGNPATH_ORGANIZATION_ID` | ID dell'organizzazione SignPath |
| variabile | `SIGNPATH_PROJECT_SLUG` | `cedro` |
| variabile | `SIGNPATH_POLICY_SLUG` | `release-signing` |

Su SignPath: progetto `cedro` collegato a GitHub (trusted build system *GitHub.com*), due artifact configuration
`exe` e `installatore` con il contenuto dei due file XML, policy `release-signing` con approvazione manuale.

Il disinstallatore `unins000.exe` che Inno Setup scrive durante l'installazione non è firmato.
