<div align="center">
  <h1>SmartSandbox</h1>
  <p><strong>Execuția proceselor cu limite de resurse și recomandări asistate de AI</strong></p>
  <p>Rust · Linux · HTTP API · HTML / CSS / JavaScript · Groq</p>
  <p>Proiect de sisteme de operare</p>
</div>

<hr>

<h2>Despre proiect</h2>
<p>SmartSandbox este un proiect realizat în Rust pentru rularea programelor externe într-un mediu cu limite configurabile de resurse. Aplicația urmărește consumul de memorie și durata execuției, colectează rezultatele și propune ajustări ale configurației. Utilizatorul poate accepta, respinge sau modifica recomandările, iar deciziile sunt păstrate pentru execuțiile următoare.</p>
<p>Proiectul combină programarea de sistem în Linux cu o interfață web și o componentă de analiză asistată de AI. Poate fi utilizat atât din terminal, cât și printr-un server HTTP conectat la un panou de control în browser.</p>

<h2>Funcționalități implementate</h2>
<ul>
  <li><strong>Execuția programelor externe:</strong> lansarea proceselor copil cu argumente, într-un director de lucru dedicat, cu un mediu de variabile simplificat.</li>
  <li><strong>Limite de resurse:</strong> configurarea spațiului de adrese, timpului CPU, dimensiunii fișierelor și limitei de procese prin mecanismele Linux <code>setrlimit</code>.</li>
  <li><strong>Controlul accesului la rețea:</strong> încercarea de creare a unui namespace de rețea separat când accesul este dezactivat.</li>
  <li><strong>Monitorizare:</strong> eșantionarea memoriei rezidente prin <code>/proc/&lt;pid&gt;/statm</code>, măsurarea duratei totale și colectarea codului de ieșire.</li>
  <li><strong>Rezultate și anomalii:</strong> capturarea <code>stdout</code> și <code>stderr</code>, semnalarea utilizării ridicate a resurselor și identificarea unor terminări prin semnal.</li>
  <li><strong>Recomandări:</strong> calcularea unor limite noi prin reguli și generarea explicației și a severității prin API-ul Groq.</li>
  <li><strong>Control uman:</strong> acceptarea, respingerea sau modificarea recomandărilor înainte de salvarea configurației pentru utilizări viitoare.</li>
  <li><strong>Persistență JSON:</strong> salvarea istoricului, recomandărilor, deciziilor și preferințelor asociate programelor.</li>
  <li><strong>Interfață web:</strong> configurarea execuției, afișarea rezultatelor, istoricului, statisticilor și preferințelor acumulate.</li>
</ul>

<h2>Cum funcționează</h2>
<ol>
  <li>Utilizatorul alege programul, argumentele și limitele de resurse.</li>
  <li>Motorul pregătește directorul <code>sandbox_root/</code> și aplică restricțiile înaintea execuției.</li>
  <li>Programul este executat, iar aplicația colectează ieșirea, durata, memoria și eventualele anomalii.</li>
  <li>Motorul de reguli calculează limitele recomandate. Dacă este configurată cheia Groq, componenta AI solicită o explicație și o severitate folosind și contextul istoric.</li>
  <li>Utilizatorul acceptă, respinge sau modifică propunerea.</li>
  <li>Rezultatul și decizia sunt salvate în <code>history.json</code>. Configurațiile aprobate pot deveni punctul de plecare pentru rulările următoare.</li>
</ol>
<p><strong>Aplicarea recomandării nu relansează automat procesul.</strong> Configurația aprobată este păstrată pentru utilizări ulterioare.</p>

<h2>Arhitectura proiectului</h2>
<table>
  <tr><th>Fișier</th><th>Responsabilitate</th></tr>
  <tr><td><code>src/sandbox.rs</code></td><td>Configurarea restricțiilor, lansarea proceselor, monitorizarea și construirea rezultatului.</td></tr>
  <tr><td><code>src/ai.rs</code></td><td>Motorul de reguli, încărcarea cheii API, comunicarea cu Groq și interpretarea răspunsului.</td></tr>
  <tr><td><code>src/history.rs</code></td><td>Persistența JSON, istoricul per program, statisticile despre decizii și configurațiile preferate.</td></tr>
  <tr><td><code>src/main.rs</code></td><td>Interfața CLI pentru execuție, istoric, statistici și validarea recomandărilor.</td></tr>
  <tr><td><code>src/server.rs</code></td><td>Serverul HTTP și endpoint-urile folosite de interfața web.</td></tr>
  <tr><td><code>web/index.html</code></td><td>Panoul web, stilurile CSS și logica JavaScript pentru comunicarea cu API-ul.</td></tr>
  <tr><td><code>cpu_test.py</code></td><td>Scenariu de solicitare CPU printr-o buclă infinită.</td></tr>
  <tr><td><code>memory_test.py</code></td><td>Scenariu de alocare progresivă a memoriei.</td></tr>
  <tr><td><code>Cargo.toml</code> / <code>Cargo.lock</code></td><td>Configurarea pachetului Rust și a dependențelor.</td></tr>
</table>

<h2>Tehnologii utilizate</h2>
<ul>
  <li><strong>Rust, ediția 2021:</strong> implementarea backendului și a aplicației CLI.</li>
  <li><strong>libc și nix:</strong> limite de resurse și namespace-uri Linux.</li>
  <li><strong>Serde și serde_json:</strong> serializarea configurațiilor, rezultatelor și istoricului.</li>
  <li><strong>tiny_http:</strong> serverul HTTP.</li>
  <li><strong>reqwest:</strong> cererile HTTP către Groq.</li>
  <li><strong>chrono și uuid:</strong> marcaje temporale și identificatori pentru execuții.</li>
  <li><strong>HTML, CSS și JavaScript:</strong> interfața web fără etapă de compilare frontend.</li>
</ul>
<p><code>seccompiler</code> este declarat în dependențe, dar versiunea analizată nu instalează un filtru seccomp.</p>

<h2>Cerințe și compilare</h2>
<ul>
  <li>Un mediu Linux cu sistemul <code>/proc</code> disponibil. Pe Windows este necesar un mediu Linux, de exemplu WSL2; proiectul folosește API-uri Unix și nu rulează nativ în Windows.</li>
  <li>Rust și Cargo, cu suport pentru ediția 2021, plus un toolchain de compilare C și dependențele OpenSSL necesare backendului TLS.</li>
  <li>Python 3 pentru scenariile demonstrative și pentru servirea locală opțională a interfeței web.</li>
  <li>O cheie Groq pentru componenta de analiză AI.</li>
</ul>
<p>Din directorul proiectului:</p>
<pre><code>cargo build --release --bins</code></pre>

<h2>Configurarea componentei AI</h2>
<p>În terminalul Linux din care pornești aplicația, configurează cheia personală:</p>
<pre><code>export GROQ_API_KEY="CHEIA_TA_GROQ"</code></pre>
<p>Aplicația caută cheia, în ordine, în variabila <code>GROQ_API_KEY</code>, în <code>api_key.txt</code> și apoi într-o linie <code>GROQ_API_KEY=...</code> din <code>.env</code>. Fișierele locale cu chei nu trebuie publicate.</p>
<p>Modelul implicit din cod este <code>llama-3.3-70b-versatile</code>. Numele modelului poate fi schimbat prin variabila <code>AI_MODEL</code>, în funcție de modelele disponibile în contul Groq.</p>
<p>Fără cheie, interfața web poate primi rezultatul execuției fără recomandare. În CLI, lipsa cheii oprește fluxul după execuție, înaintea dialogului de decizie și a salvării în istoric. Dacă există o cheie, dar apelul către model eșuează, analiza revine la explicația și severitatea motorului de reguli.</p>

<h2>Utilizare din terminal</h2>
<pre><code># Execută un program
cargo run --release --bin sandbox -- /bin/echo Salut

# Afișează ultimele 20 de înregistrări
cargo run --release --bin sandbox -- history

# Afișează statisticile
cargo run --release --bin sandbox -- stats</code></pre>
<p>După analiză, CLI-ul oferă opțiunile <code>a</code> pentru acceptare, <code>r</code> pentru respingere și <code>m</code> pentru modificare. În varianta CLI, modificarea manuală solicită valorile RAM și CPU.</p>

<h2>Utilizare prin interfața web</h2>
<p>Pornește backendul din rădăcina proiectului, în terminalul în care ai configurat cheia:</p>
<pre><code>cargo run --release --bin sandbox_server</code></pre>
<p>Într-un al doilea terminal, tot din rădăcina proiectului, servește interfața:</p>
<pre><code>python3 -m http.server 8000 --bind 127.0.0.1 --directory web</code></pre>
<p>Deschide <code>http://localhost:8000</code>. Interfața trimite cererile către <code>http://localhost:7878</code>; backendul servește API-ul, iar fișierul HTML este servit separat.</p>
<p>Panoul permite alegerea programului și a argumentelor, ajustarea limitelor, controlul opțiunilor de rețea și scriere, consultarea rezultatelor și validarea recomandărilor. Secțiunile <em>History</em> și <em>Analytics</em> prezintă execuțiile salvate, rata de succes și datele despre feedback. Metricile sunt afișate după răspunsul cererii de execuție; interfața nu primește un flux continuu de măsurători.</p>

<h2>Configurația implicită a motorului</h2>
<table>
  <tr><th>Parametru</th><th>Valoare</th></tr>
  <tr><td><code>ram_mb</code></td><td>128 MB</td></tr>
  <tr><td><code>cpu_time_sec</code></td><td>60 secunde</td></tr>
  <tr><td><code>disk_mb</code></td><td>50 MB per fișier, când scrierea este permisă</td></tr>
  <tr><td><code>max_processes</code></td><td>10</td></tr>
  <tr><td><code>allow_network</code></td><td>false</td></tr>
  <tr><td><code>allow_file_write</code></td><td>false</td></tr>
  <tr><td><code>timeout_sec</code></td><td>60 secunde</td></tr>
</table>
<p>Interfața web pornește cu 5 secunde CPU și 10 secunde timeout și trimite propria configurație. Aceste valori diferă de valorile implicite ale motorului.</p>

<h2>Recomandări și feedback</h2>
<p>Limitele numerice și acțiunea recomandată sunt stabilite de motorul de reguli:</p>
<ul>
  <li>RAM: limita este triplată la detectarea unui <code>MemoryError</code> sau a unei terminări interpretate ca problemă de memorie și dublată la utilizare de peste 80%; în rest este păstrată.</li>
  <li>CPU: limita este dublată la detectarea depășirii sau a unui raport de utilizare de peste 80%. Pentru o execuție reușită sub 20%, este recalculată la aproximativ dublul duratei observate, rotunjit în sus, cu minimum o secundă.</li>
  <li>Fișiere: limita este dublată dacă terminarea este interpretată ca depășire a dimensiunii permise.</li>
  <li>Timeout: este păstrat la eșec; la succes devine timpul CPU recomandat plus 5 secunde.</li>
</ul>
<p>Modelul lingvistic primește rezultatul, limitele propuse și contextul istoric, apoi produce explicația și severitatea. Preferințele utilizatorului sunt reprezentate prin contoare, medii ale corecțiilor și ultimele cinci configurații aprobate. Cea mai recentă configurație preferată poate fi reutilizată la următoarea execuție a aceluiași program.</p>
<p>Acest mecanism păstrează feedback și îl include în contextul AI; nu antrenează un model nou și nu înlocuiește regulile numerice. Istoricul este grupat după numele programului, nu după combinația program–argumente.</p>

<h2>API HTTP</h2>
<table>
  <tr><th>Metodă</th><th>Endpoint</th><th>Rol</th></tr>
  <tr><td>POST</td><td><code>/api/run</code></td><td>Execută un program și returnează rezultatul și recomandarea disponibilă.</td></tr>
  <tr><td>POST</td><td><code>/api/decision</code></td><td>Înregistrează decizia și salvează execuția în istoric.</td></tr>
  <tr><td>GET</td><td><code>/api/history</code></td><td>Returnează înregistrările salvate.</td></tr>
  <tr><td>GET</td><td><code>/api/stats</code></td><td>Returnează numărul de rulări, rata de succes și rata de acceptare, calculată raportat la totalul înregistrărilor.</td></tr>
  <tr><td>GET</td><td><code>/api/learning/&lt;program&gt;</code></td><td>Returnează preferințele și statisticile de feedback pentru programul codificat în URL.</td></tr>
</table>
<p>Exemplu de corp JSON pentru <code>POST /api/run</code>:</p>
<pre><code>{
  "program": "/bin/echo",
  "args": ["Salut din SmartSandbox"],
  "config": {
    "ram_mb": 128,
    "cpu_time_sec": 5,
    "disk_mb": 50,
    "max_processes": 10,
    "allow_network": false,
    "allow_file_write": false,
    "timeout_sec": 10
  }
}</code></pre>
<p>Deciziile acceptate sunt <code>accept</code>, <code>reject</code> și <code>modify</code>. Pentru modificare se poate transmite configurația în <code>new_config</code>. Răspunsurile API folosesc câmpul <code>success</code> și, după caz, <code>data</code> sau <code>error</code>.</p>

<h2>Scenarii demonstrative</h2>
<p>Scripturile Python incluse solicită intenționat resursele pentru observarea restricțiilor. Folosește căi absolute, deoarece procesul copil rulează din <code>sandbox_root/</code>:</p>
<pre><code>cargo run --release --bin sandbox -- /usr/bin/python3 "$(pwd)/cpu_test.py"
cargo run --release --bin sandbox -- /usr/bin/python3 "$(pwd)/memory_test.py"</code></pre>
<p>Aceste scripturi sunt demonstrații de consum CPU și RAM, nu o suită automată de teste. Scenariul CPU nu produce ieșire, iar limitarea CPU poate termina procesul înainte ca timeout-ul aplicației să fie verificat.</p>

<h2>Limitele versiunii actuale</h2>
<ul>
  <li><strong>Izolarea sistemului de fișiere:</strong> schimbarea directorului de lucru nu implementează <code>chroot</code>, un container sau blocarea accesului la fișierele gazdei.</li>
  <li><strong>Rețea:</strong> crearea namespace-ului necesită privilegii corespunzătoare. Dacă operația eșuează, codul afișează un avertisment și continuă; rețeaua poate rămâne accesibilă.</li>
  <li><strong>Limite Linux:</strong> <code>RLIMIT_AS</code> limitează spațiul de adrese virtual, iar <code>RLIMIT_FSIZE</code> dimensiunea unui fișier, nu totalul spațiului ocupat. Valoarea zero pentru scriere nu reprezintă un sistem de fișiere complet read-only. <code>RLIMIT_NPROC</code> are semantica și excepțiile Linux, inclusiv cele asociate privilegiilor și utilizatorului.</li>
  <li><strong>Măsurători:</strong> memoria este eșantionată pentru procesul principal, cu pagini presupuse de 4096 de octeți. În anumite terminări, valoarea raportată este estimată la 95% din limită. Câmpul <code>cpu_time_used_sec</code> și regulile CPU folosesc durata totală observată, nu o măsurare distinctă a timpului CPU.</li>
  <li><strong>Timeout și ieșire:</strong> citirea blocantă și secvențială a fluxurilor poate întârzia verificarea timeout-ului sau produce blocaje. Capturarea este limitată la 500 de linii pe flux, iar firul de monitorizare nu are în prezent un mecanism explicit de oprire.</li>
  <li><strong>Server:</strong> ascultă pe <code>0.0.0.0:7878</code>, permite CORS pentru orice origine și nu implementează autentificare. Poate lansa programe cu drepturile utilizatorului serverului; folosește-l într-un mediu local controlat, fără expunere publică.</li>
  <li><strong>Decizii:</strong> backendul păstrează o singură execuție în așteptare; o nouă execuție o poate înlocui. Câmpul <code>run_id</code> primit la decizie nu este validat față de execuția în așteptare.</li>
</ul>
<p>Proiectul demonstrează mecanisme de control al resurselor și analiză a execuției. Versiunea actuală nu oferă izolarea completă necesară rulării codului ostil.</p>

<h2>Concepte explorate</h2>
<p>Proiectul reunește gestionarea proceselor copil, apeluri de sistem Linux, limite de resurse, namespace-uri, monitorizare prin <code>/proc</code>, fire de execuție și sincronizare, serializare JSON, API-uri HTTP, integrarea unui model lingvistic și un flux de feedback în care utilizatorul controlează configurațiile viitoare.</p>
