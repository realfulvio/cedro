# Task8 — cinque temi desktop e migrazione delle preferenze

Base: `db3a1a5`, ramo `codex/android-allinea-0.53`. Implementazione completata; verifica eseguibile Rust/GUI delegata al controller secondo il brief. Nessun comando cargo, rustc, rustfmt/fmt, adb o carico remoto eseguito.

## Implementazione

- `src/ui/theme.rs`: `THEMES` contiene soltanto Cedro Melanzana (`cedro`), Notte, Bosco, Carta e Cedro for Labs (`cedro-labs`). `ThemeId::from_key` è l'ingresso desktop: chiavi ritirate, vuote o sconosciute danno Melanzana. Gli enum e le palette precedenti restano disponibili al codice condiviso. `ANDROID_THEMES` conserva esattamente i nove temi già presenti sul telefono, nell'ordine originale: quattro tinte più i cinque aspetti precedenti; Bondeno e Labs continuano a essere esclusi. `from_android_key` non dipende da `target_os`.
- `android/src/ui.rs`, `android/src/glue.rs`, `android/examples/vista.rs`: selettore, rilettura di `tema.txt` ed esempi usano esplicitamente il catalogo/parser Android. L'eccezione storica `tema-045` resta invariata. Il renderer compilato su Linux continua quindi a riconoscere i temi Android precedenti.
- `src/settings.rs`: `read_settings` e `write_settings` attraversano i nuovi helper con directory esplicita, gli stessi esercitati dai test senza modificare HOME o configurazioni personali. La lettura desktop normalizza e salva il tema prima di restituire le impostazioni; anche un successivo salvataggio di una chiave ritirata scrive `cedro`. La normalizzazione cambia esclusivamente `ui_theme`. Sostituita la vecchia migrazione dello schema che poteva modificare `simple`: ora `simple`, `view_asked`, qualità e ogni altra preferenza conosciuta restano invariati anche dagli schemi precedenti. `ui_schema=3` descrive il catalogo ridotto, senza cambiare la versione dell'app.
- Il marker della vecchia edizione Bondeno conduce a Melanzana; viene consumato soltanto dopo un salvataggio riuscito. Il marker Labs continua a funzionare (e ha precedenza se sono presenti entrambi). La vista Completa viene impostata soltanto per una nuova installazione Labs, mai durante la migrazione di impostazioni esistenti. In caso di errore di persistenza, la lettura restituisce comunque un tema valido e riprova alla lettura successiva.
- Il livello generico `Settings::parse/load_from/merge_into/save_to` resta un serializzatore delle preferenze condivise: la politica desktop vive nei percorsi reali read/write, mentre su target Android queste migrazioni desktop non vengono eseguite. Rimangono conservate le chiavi sconosciute. Corretto `merge_into` per eliminare il BOM prima del filtraggio: una chiave `ui_theme` in prima riga non sopravvive duplicata alla migrazione.
- `src/edizione.rs`: eliminato il bool Bondeno derivato indipendentemente dalla chiave grezza; l'edizione segue esclusivamente il tema risolto nel catalogo desktop. `ui_theme=bondeno` non può più riaccendere stemmi, titolo o carattere dell'edizione.
- `src/ui/aspetto.rs`: anche il tema iniziale della pagina viene risolto nel catalogo desktop. Le due finestre Linux/Windows usano già questa pagina e lo stesso catalogo per mouse, tastiera e anteprime. Aggiornati gli esempi desktop che preselezionavano un tema ritirato.
- README italiano/inglese e guida in-app descrivono i cinque temi, la migrazione e la distinzione Android. Le tre stringhe cambiate nella guida sono tradotte in en/de/es/fr/pt. L'animazione Aspetto prende nomi e tinte dal catalogo attuale. Aggiornati soltanto i commenti obsoleti dei tre installer; nessun comportamento di packaging o workflow modificato.

## Test comportamentali scritti — NON ESEGUITI

Sette test in `settings::desktop_theme_tests`:

1. `startup_migrates_retired_themes_and_preserves_every_other_preference`: percorso reale di avvio su file temporanei; tutte le sei chiavi ritirate, chiave sconosciuta e vuota, schema assente/1/2/3 e entrambe le viste (64 combinazioni). Confronta l'intero `Settings` dopo aver sostituito soltanto il tema atteso, verifica persistenza, chiavi sconosciute caso/cronologia/web, assenza di duplicati e idempotenza.
2. `startup_migrates_bom_and_utf16_configs_without_duplicate_theme_keys`: chiave Bondeno in prima riga, UTF-8 con BOM e UTF-16 LE con BOM; persistenza UTF-8, una sola chiave tema, vista e preferenze estranee conservate.
3. `valid_desktop_themes_round_trip_on_startup_and_save`: tutti i cinque temi e entrambe le viste, avvio da schema precedente e salvataggio, qualità e chiavi sconosciute intatte.
4. `saving_a_retired_theme_never_persists_it_or_loses_unknown_lines`: percorso reale di salvataggio per sei ritirati/sconosciuto/vuoto, entrambe le viste; verifica anche che l'oggetto del chiamante resti invariato.
5. `bondeno_installer_marker_migrates_to_melanzana_and_keeps_the_chosen_view`: vecchia edizione con impostazioni esistenti e nuova cartella; conserva vista e preferenze, consuma il marker.
6. `labs_installer_marker_still_works_and_preserves_existing_preferences`: marker simultanei, precedenza Labs, preferenze esistenti e nuova installazione Completa.
7. `failed_marker_save_keeps_the_marker_for_retry`: errore filesystem reale usando una directory al posto di config.txt; il marker rimane e la lettura successiva riprova con successo.

Inoltre:

- `ui::theme::tests::desktop_retires_legacy_themes_without_changing_android_keys`: elenchi esatti, fallback dei ritirati desktop e roundtrip delle nove chiavi Android con spazi/newline anche sull'host.
- `edizione::tests::la_vecchia_chiave_non_riattiva_l_edizione`: chiave Bondeno grezza, veste disattivata, nessun nome edizione e titolo standard.
- `ui::temi_regressioni::android_keeps_legacy_theme_selection_save_reload_and_host_rendering` nel crate Android: seleziona davvero ciascuno dei nove temi mediante l'azione UI, verifica l'effetto di salvataggio, rilegge la chiave con il parser runtime e disegna una schermata a 360×800 con i caratteri del tema; verifica controlli presenti e tema conservato. Verifica anche fallback Bondeno/Labs/sconosciuto/vuoto.
- Adattato il test esistente di serializzazione per usare due temi desktop validi e lo schema 3. Nessun test specchio della sola implementazione: i nuovi casi verificano file, migrazione, persistenza, contratto piattaforma ed effetti UI.

## Verifiche eseguite sul notebook

`git diff --check` → exit 0, nessun output.

`bash -n installer/install-linux.sh installer/build-linux.sh` → exit 0, nessun output. Esegue soltanto il parser bash; non avvia gli installer/build.

Controllo statico inline con `python3 - <<'PY'` (script riproducibile sotto) → exit 0:

```text
PASS: desktop 5 temi; Android 9 invariati; enum, palette, tinte, attività e dark invariati
PASS: parser/catalogo Android espliciti nel runtime e negli esempi, indipendenti dal target host
PASS: percorsi reali read/write collegati alla normalizzazione; assenti reset della vista ed effetto Bondeno indipendente
PASS: 15/15 traduzioni della guida presenti e non duplicate (en/de/es/fr/pt)
PASS: installer modificati solo nei commenti; workflow, manifest e versione invariati
PASS: 7 test desktop di migrazione/roundtrip/errore e regressione Android selezione/salvataggio/renderer presenti; NON ESEGUITI
```

Questi risultati sono controlli del sorgente, non prove di compilazione né esecuzioni dei test Rust.

## Gate da eseguire sul mini-PC a cura del controller

Dopo aver trasferito questa modifica in una copia coordinata dal controller, dalla radice di quella copia (non dalla copia APK immutabile db3a1a5 ancora in uso):

```sh
cargo test --lib settings:: -- --test-threads=1
cargo test --lib ui::theme::tests:: -- --test-threads=1
cargo test --lib edizione::tests:: -- --test-threads=1
cargo test --manifest-path android/Cargo.toml --lib ui::temi_regressioni:: -- --test-threads=1
cargo check --examples
cargo check --manifest-path android/Cargo.toml --examples
```

Dopo i mirati, suite `cargo test --lib` e `cargo test --manifest-path android/Cargo.toml --lib` una volta secondo i limiti di concorrenza del controller. Nessuno di questi comandi è stato lanciato dall'implementer. Anche check/build Android ARM64 e build Windows rimangono NON ESEGUITI per questa modifica.

Collaudo GUI proposto: Linux e Windows con configurazione isolata; aprire Aspetto e verificare esattamente cinque temi sia con mouse sia con frecce, provare le due viste, riavviare; ripetere da config con Bondeno e con marker della vecchia edizione, controllando font/titolo/stemmi e qualità conservati. Sul telefono e nel renderer host Android, selezionare i cinque temi precedenti e riavviare mantenendo il tema scelto (rispettare la migrazione storica tema-045 già esistente). Nessun intervento sulle preferenze personali richiesto all'implementer.

## Self-review e limiti

Riletto il diff di implementazione e verificati i percorsi reali Windows/Linux: caricamento `from_key`, pagina Aspetto condivisa, salvataggio `write_settings`, stato globale dell'edizione. I vecchi enum, renderer e asset restano intenzionalmente per il codice condiviso; nessuna palette/rendering conservata è stata riscritta. `Screen::request_appearance` resta utilizzabile da test di rendering con enum espliciti: non è un selettore né un caricamento dalle preferenze, che passano tutti dal catalogo ristretto.

La self-review ha individuato il BOM in prima riga come possibile duplicazione di una chiave ritirata durante merge; corretto e coperto dal nuovo test sulle codifiche. Verificato che l'aggiornamento dalle vecchie impostazioni non attraversi più la migrazione storica che impostava Semplice, e che il flag Bondeno non abbia un ingresso indipendente dal tema valido. Nessun dubbio architetturale residuo identificato nel sorgente.

Limite materiale: i test Rust non sono stati compilati né eseguiti, come richiesto; correttezza eseguibile, UI reali e compilazione per piattaforma restano gate del controller/reviewer. Nessuna release o pubblicazione desktop/Android eseguita. Nessuna modifica a configurazioni personali, processi remoti o workflow. I report Task3/Task4 del controller e gli untracked PROMPT-CODEX-tracciadoc.md e foto-riga/ restano fuori dal commit.

## Script statico eseguito

```python
from pathlib import Path
import re, subprocess
base=lambda path:subprocess.check_output(['git','show','db3a1a5:'+path],text=True)
theme=Path('src/ui/theme.rs').read_text();android=Path('android/src/ui.rs').read_text();settings=Path('src/settings.rs').read_text()
def array(s,name):
 match=re.search(r'pub const '+name+r': \[ThemeId; \d+\] = \[(.*?)\];',s,re.S)
 assert match,name
 return re.findall(r'ThemeId::(\w+)',match[1])
def body(s,head):
 start=s.index(head);brace=s.index('{',start);depth=1;end=brace+1
 while depth:
  depth+=(s[end]=='{')-(s[end]=='}');end+=1
 return s[start:end]
assert array(theme,'THEMES')==['Cedro','CedroNotte','CedroBosco','CedroCarta','CedroLabs']
assert array(theme,'ANDROID_THEMES')==array(base('android/src/ui.rs'),'TEMI')
assert body(theme,'pub enum ThemeId')==body(base('src/ui/theme.rs'),'pub enum ThemeId')
for method in ['pub fn palette(', 'pub fn tint(', 'pub fn activities(', 'pub fn dark(']:
 assert body(theme,method)==body(base('src/ui/theme.rs'),method),method
print('PASS: desktop 5 temi; Android 9 invariati; enum, palette, tinte, attività e dark invariati')
assert 'pub const TEMI: [ThemeId; 9] = cedro::ui::theme::ANDROID_THEMES;' in android
assert 'crate::ui::tema_da_chiave(&t)' in Path('android/src/glue.rs').read_text()
for p in Path('android').rglob('*.rs'):
 assert 'ThemeId::from_key' not in p.read_text(),p
 assert 'theme::THEMES' not in p.read_text(),p
assert 'THEMES.into_iter().find(' in body(theme,'pub fn from_key(')
assert 'ANDROID_THEMES.into_iter().find(' in body(theme,'pub fn from_android_key(')
print('PASS: parser/catalogo Android espliciti nel runtime e negli esempi, indipendenti dal target host')
assert 'read_settings_in(&crate::config_dir())' in body(settings,'pub fn read_settings(')
assert 'write_settings_in(s, &crate::config_dir())' in body(settings,'pub fn write_settings(')
assert 'normalizza_tema_desktop' in body(settings,'fn read_settings_in(')
assert 'normalizza_tema_desktop' in body(settings,'fn write_settings_in(')
assert 's.simple' not in body(settings,'fn normalizza_tema_desktop(')
assert 'al_nuovo_aspetto' not in settings
assert 'BONDENO.store' not in Path('src/edizione.rs').read_text()
print('PASS: percorsi reali read/write collegati alla normalizzazione; assenti reset della vista ed effetto Bondeno indipendente')
new_keys=["In Impostazioni, Aspetto: Cedro Melanzana, Notte, Bosco, Carta e Cedro for Labs, la vista Semplice (solo l'essenziale) o Completa (tutti i comandi) e i caratteri più grandi. Questa Guida cambia insieme a loro.", 'Cinque temi desktop','Melanzana, Notte, Bosco, Carta e Cedro for Labs']
for lang in ['en','de','es','fr','pt']:
 lines=[line.split('\t',1) for line in Path('i18n',lang+'.tsv').read_text().splitlines()]
 for key in new_keys:
  matches=[parts for parts in lines if parts[0]==key]
  assert len(matches)==1 and len(matches[0])==2 and matches[0][1].strip(),(lang,key)
print('PASS: 15/15 traduzioni della guida presenti e non duplicate (en/de/es/fr/pt)')
for p in ['installer/cedro.iss','installer/build-linux.sh','installer/install-linux.sh']:
 def executable(text):
  return [line for line in text.splitlines() if not line.lstrip().startswith(('#',';','//'))]
 assert executable(Path(p).read_text())==executable(base(p)),p
changed=subprocess.check_output(['git','diff','--name-only'],text=True).splitlines()
assert not any(p.startswith(('.github/','.cargo/')) or p.endswith('Cargo.toml') for p in changed)
print('PASS: installer modificati solo nei commenti; workflow, manifest e versione invariati')
assert len(re.findall(r'#\[test\]',settings.split('mod desktop_theme_tests {',1)[1]))==7
assert 'android_keeps_legacy_theme_selection_save_reload_and_host_rendering' in android
print('PASS: 7 test desktop di migrazione/roundtrip/errore e regressione Android selezione/salvataggio/renderer presenti; NON ESEGUITI')
```
