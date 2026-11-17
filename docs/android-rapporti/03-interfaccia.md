# Interfaccia Android A

L'utente ha approvato la variante A dell'[anteprima](../android-anteprime/rework/index.html) e autorizzato il completamento e la pubblicazione. I commit `39622bd` e `a0740da` portano la proposta in Rust: Inizio con sei attività colorate, tre destinazioni inferiori, file compatto, catalogo completo per famiglie, strumenti separati, pulsante principale fisso e foglio Condividi con anteprima. Le quattro tinte e i cinque temi precedenti restano disponibili.

I dati dimostrativi appartengono soltanto all'esempio host. Nell'app il foglio legge il file ricevuto, il catalogo usa gli identificatori effettivi del motore e la coda contiene i file selezionati. La scelta del formato aggiorna insieme attività e indice, correggendo il disallineamento fra spicchio e conversione osservato nella 0.45.

La revisione indipendente ha segnalato bersagli troppo stretti per opzioni come «No» e «1». Il fix assicura 48×48 dp negli spazi disponibili, corregge l'altezza restituita e calcola il piede del foglio dalla stessa misura del pulsante multilinea. La seconda revisione statica non ha rilevato altre regressioni nel fix.

| Verifica | Stato e limite |
|---|---|
| Check host intermedio su minipc | Passato, 1 min 47 s. Non certifica gli ultimi ritocchi. |
| Esempio Rust A intermedio | 69 PNG prodotti; compilazione e linking 8 min 27 s. IT/DE, 360×800, 360×640, 800×400 e tinte. |
| Confronto visivo intermedio | Impianto coerente con l'HTML; ritocchi successivi a centratura, titoli tradotti e geometria del foglio. |
| Review del commit UI e fix | Prima review: 1 Important e 3 Minor. Important e 2 Minor corretti; warning vendor separati dal codice UI. Re-review statica approvata. |
| Test sorgente | Navigazione, catalogo, selezione PDF/AVIF e dispatch, bersagli, wrapping, CTA multilinea; esecuzione ancora da registrare. |
| Check/test/render del commit finale | Da eseguire dopo l'esclusiva temporanea del minipc per il collaudo TracciaDoc. |
| APK e schermo Mi 9T | Da eseguire dopo le integrazioni; nessuna equivalenza presunta fra render host e telefono. |

I controlli finali sono obbligatori prima della release. FontScale Android, accessibilità, workspace Labs, comandi della trascrizione e stato dei componenti web appartengono alle integrazioni successive; non vengono dichiarati terminati da questo rapporto.
