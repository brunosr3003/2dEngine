// O site em dois idiomas, sem duplicar a página.
//
// O caminho óbvio seria manter `index.en.html` ao lado do `index.html`. Foi
// recusado: são duas páginas pra editar a cada mudança de preço, de botão ou
// de FAQ, e a segunda sempre fica atrás. Aqui a página continua UMA, escrita
// em português, e a tradução troca o TEXTO dos nós — a chave do dicionário é a
// própria frase em português, igual ao que o jogo faz em `shared::idioma`.
//
// Consequências que valem saber:
//
// - sem JavaScript, o site inteiro funciona em português (o download também);
// - frase sem verbete fica em português, e não vazia;
// - a escolha fica no `localStorage`, e a PRIMEIRA visita respeita o idioma
//   do navegador — quem abre de fora do Brasil já cai no inglês.

(function () {
  'use strict';

  // A chave é a frase exata do HTML, com os espaços já normalizados.
  var EN = {
    // cabeçalho e navegação
    'UM MUNDO PARA EXPLORAR': 'A WORLD TO EXPLORE',
    'Ir para downloads': 'Skip to downloads',
    'Tempest, início': 'Tempest, home',
    'Navegação principal': 'Main navigation',
    'O mundo': 'The world',
    'Como jogar': 'How to play',
    'Downloads': 'Downloads',
    // herói
    'Captura do Tempest: personagem, vilarejo e ilhas cercadas pelo mar':
      'Tempest screenshot: character, village and islands surrounded by the sea',
    'MMORPG · EXPLORAÇÃO · AVENTURA': 'MMORPG · EXPLORATION · ADVENTURE',
    'O horizonte': 'The horizon',
    'é só o': 'is only the',
    'começo.': 'beginning.',
    'Uma ilha para descobrir. Uma arma para dominar.': 'An island to discover. A weapon to master.',
    'Mil histórias esperando pela sua.': 'A thousand stories waiting for yours.',
    'Baixar para Windows': 'Download for Windows',
    'Outras plataformas': 'Other platforms',
    'Windows 64 bits · Entre com Google ou com sua conta':
      'Windows 64-bit · Sign in with Google or with your account',
    '01 / O PRIMEIRO DESTINO': '01 / THE FIRST DESTINATION',
    'Seu lugar no mundo.': 'Your place in the world.',
    'Imagem capturada no jogo': 'Captured in game',
    // o mundo
    'ESCREVA SUA PRÓPRIA ROTA': 'CHART YOUR OWN COURSE',
    'Pequenos blocos.': 'Small blocks.',
    'Grandes aventuras.': 'Big adventures.',
    'Da primeira coleta ao próximo desafio,': 'From your first gather to your next challenge,',
    'sempre existe algo além da costa.': "there is always something beyond the coast.",
    'Explore o arquipélago': 'Explore the archipelago',
    'Atravesse vilarejos, bosques e ilhas. Encontre missões e descubra novos caminhos.':
      'Cross villages, groves and islands. Find quests and discover new paths.',
    'Encontre seu estilo': 'Find your style',
    'Escolha sua arma, desenvolva habilidades e prepare seus equipamentos para a próxima batalha.':
      'Choose your weapon, grow your skills and ready your gear for the next battle.',
    'Faça parte do mundo': 'Become part of the world',
    'Colete recursos, enfrente criaturas e aventure-se nas zonas de PvP da Ilha Mágica.':
      'Gather resources, take on creatures and venture into the PvP zones of the Magic Island.',
    // downloads
    'SUA AVENTURA, NA SUA TELA': 'YOUR ADVENTURE, ON YOUR SCREEN',
    'Escolha onde jogar.': 'Choose where to play.',
    'Baixe a versão atual do Tempest.': 'Download the current version of Tempest.',
    'O próximo passo é dentro do jogo.': 'The next step is inside the game.',
    'DESKTOP': 'DESKTOP',
    'MOBILE': 'MOBILE',
    'TESTFLIGHT': 'TESTFLIGHT',
    'Teclado, mouse e um mundo inteiro.': 'Keyboard, mouse and a whole world.',
    '64 bits · Pacote ZIP': '64-bit · ZIP package',
    'Extraia o ZIP e abra': 'Extract the ZIP and open',
    'Leve sua aventura com você.': 'Take your adventure with you.',
    'Baixar APK': 'Download APK',
    'Abra o arquivo no celular para instalar.': 'Open the file on your phone to install it.',
    'Feito para o seu Apple Silicon.': 'Built for your Apple Silicon.',
    'Apple Silicon · ZIP': 'Apple Silicon · ZIP',
    'Baixar para Mac': 'Download for Mac',
    'Explore no seu computador Linux.': 'Explore on your Linux machine.',
    'x86_64 · ZIP': 'x86_64 · ZIP',
    'Baixar para Linux': 'Download for Linux',
    'Extraia o ZIP e execute': 'Extract the ZIP and run',
    '. Requer OpenGL e ALSA.': '. Requires OpenGL and ALSA.',
    'iPhone e iPad': 'iPhone and iPad',
    'Acesso de teste por convite.': 'Test access by invitation.',
    'iOS 15 ou superior': 'iOS 15 or later',
    'Como jogar no iPhone': 'How to play on iPhone',
    'Participantes convidados atualizam pelo TestFlight.':
      'Invited testers update through TestFlight.',
    'Os pacotes incluem os arquivos necessários para jogar. No computador, mantenha os arquivos extraídos juntos.':
      'The packages include everything you need to play. On a computer, keep the extracted files together.',
    'Veja as novidades da atualização.': "See what's new in this update.",
    // como jogar
    'DO DOWNLOAD AO PRIMEIRO PASSO': 'FROM DOWNLOAD TO FIRST STEP',
    'Entre. Explore.': 'Sign in. Explore.',
    'Encontre sua história.': 'Find your story.',
    'Baixe e abra o jogo': 'Download and open the game',
    'Escolha o pacote da sua plataforma e siga as instruções acima.':
      'Pick the package for your platform and follow the instructions above.',
    'Entre na sua conta': 'Sign in to your account',
    'Use o botão “Entrar com Google” ou seu usuário e senha. Você também pode criar uma conta no jogo.':
      'Use the “Sign in with Google” button, or your username and password. You can also create an account in the game.',
    'Escolha seu personagem': 'Choose your character',
    'Entre no mundo e siga suas primeiras missões.':
      'Enter the world and follow your first quests.',
    'PRONTO PARA O DESKTOP': 'READY FOR DESKTOP',
    'O mundo nas suas mãos.': 'The world in your hands.',
    'Mover personagem': 'Move your character',
    'Usar habilidades': 'Use skills',
    'ESPAÇO': 'SPACE',
    'Atacar': 'Attack',
    'Segurar botão direito + arrastar': 'Hold right button + drag',
    'Girar câmera': 'Rotate the camera',
    'Roda do mouse': 'Mouse wheel',
    'Aproximar / afastar': 'Zoom in / out',
    'Pular / defender': 'Jump / block',
    'No celular, use o direcional e os botões na tela.':
      'On a phone, use the on-screen stick and buttons.',
    // perguntas
    'Antes de embarcar.': 'Before you set sail.',
    'Como instalar no Windows?': 'How do I install on Windows?',
    'Baixe o ZIP, clique com o botão direito e escolha “Extrair tudo”. Abra a pasta extraída e execute Tempest.exe. Mantenha a pasta assets junto do executável.':
      'Download the ZIP, right-click it and choose “Extract all”. Open the extracted folder and run Tempest.exe. Keep the assets folder next to the executable.',
    'Já tenho uma versão antiga. Como atualizo?': 'I have an older version. How do I update?',
    'Feche o jogo e baixe o pacote atual. No computador, extraia-o em uma nova pasta. No Android, instale o APK sobre a versão anterior. Seus personagens ficam vinculados à sua conta.':
      'Close the game and download the current package. On a computer, extract it into a new folder. On Android, install the APK over the previous version. Your characters stay tied to your account.',
    'Qual Mac é compatível?': 'Which Mac is supported?',
    'Esta versão é nativa para Macs com Apple Silicon, como M1, M2, M3 e M4. Ela não é um pacote para Macs Intel.':
      'This version is native to Apple Silicon Macs, such as M1, M2, M3 and M4. It is not a build for Intel Macs.',
    'O jogo fechou ou não abriu. E agora?': "The game closed, or never opened. What now?",
    'Use a versão atual e mantenha o driver gráfico do computador atualizado. Se continuar acontecendo, envie a mensagem de erro e o modelo do aparelho pelo':
      'Use the current version and keep your machine’s graphics driver up to date. If it keeps happening, send the error message and your device model through',
    'suporte': 'support',
    // rodapé
    'Nos vemos do outro lado da costa.': 'See you on the other side of the coast.',
    'Suporte': 'Support',
    'Privacidade': 'Privacy',
    // a página do TestFlight
    'TEMPEST NO IPHONE E IPAD': 'TEMPEST ON IPHONE AND IPAD',
    'Jogue pelo TestFlight.': 'Play through TestFlight.',
    'A versão de teste para iOS está disponível para participantes convidados. Ainda não há um convite público.':
      'The iOS test build is available to invited testers. There is no public invite yet.',
    'Se você já participa, abra o TestFlight, selecione Tempest e toque em Atualizar. Novas builds podem levar algum tempo para aparecer enquanto a Apple conclui o processamento.':
      'If you are already a tester, open TestFlight, select Tempest and tap Update. New builds can take a while to appear while Apple finishes processing them.',
    'Abrir TestFlight': 'Open TestFlight',
    'Não tem o aplicativo?': "Don't have the app?",
    'Instale o TestFlight pela App Store': 'Install TestFlight from the App Store',
    'e aceite o convite recebido.': 'and accept the invite you received.',
    'Voltar aos downloads': 'Back to downloads',
    // O <title> e a <meta description>: ficam no <head>, fora do caminhador,
    // e são o que aparece na aba e no resultado de busca.
    'Tempest — Sua próxima aventura começa aqui': 'Tempest — Your next adventure starts here',
    'Tempest — TestFlight': 'Tempest — TestFlight',
    'Explore ilhas, escolha suas armas e escreva sua história em Tempest. Baixe o jogo para Windows, Android ou Mac.':
      'Explore islands, choose your weapons and write your story in Tempest. Download the game for Windows, Android or Mac.',
  };

  var CHAVE = 'tempest.idioma';
  var idioma = escolhaInicial();

  // Guardado na PRIMEIRA passada, antes de qualquer troca: é o português que
  // serve de chave, então trocar pra inglês e voltar não pode depender de um
  // dicionário inverso (que erraria em toda frase repetida).
  var nos = [];

  function escolhaInicial() {
    try {
      var salvo = localStorage.getItem(CHAVE);
      if (salvo === 'pt' || salvo === 'en') return salvo;
    } catch (e) {
      // localStorage bloqueado (janela privada, cookies negados): segue sem
      // lembrar. Não é motivo pra não traduzir.
    }
    var nav = (navigator.languages && navigator.languages[0]) || navigator.language || 'pt';
    return nav.toLowerCase().indexOf('pt') === 0 ? 'pt' : 'en';
  }

  function guarda(v) {
    try {
      localStorage.setItem(CHAVE, v);
    } catch (e) {}
  }

  /// Coleta os nós de texto e os atributos que o leitor de tela lê.
  function coleta() {
    // O <head> primeiro: título da aba e descrição de busca. O caminhador
    // abaixo só entra no <body>, e sem isto a aba continuaria em português
    // com a página inteira em inglês.
    if (document.title) nos.push({el: document, attr: 'title', pt: document.title});
    var desc = document.querySelector('meta[name="description"]');
    if (desc && desc.content) nos.push({el: desc, attr: 'content', pt: desc.content});

    var anda = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode: function (no) {
        var pai = no.parentNode;
        if (!pai) return NodeFilter.FILTER_REJECT;
        var tag = pai.nodeName;
        // Script e style têm "texto" que não é texto.
        if (tag === 'SCRIPT' || tag === 'STYLE') return NodeFilter.FILTER_REJECT;
        // O tamanho do arquivo é escrito pelo site.js a cada carga: traduzir
        // aqui competiria com ele.
        if (pai.hasAttribute && pai.hasAttribute('data-size')) return NodeFilter.FILTER_REJECT;
        return no.nodeValue.trim() ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      },
    });
    for (var no = anda.nextNode(); no; no = anda.nextNode()) {
      nos.push({no: no, pt: no.nodeValue});
    }
    var atributos = ['aria-label', 'title', 'content'];
    var todos = document.querySelectorAll('[aria-label],[title]');
    for (var i = 0; i < todos.length; i++) {
      for (var k = 0; k < atributos.length; k++) {
        var v = todos[i].getAttribute(atributos[k]);
        if (v && v.trim()) nos.push({el: todos[i], attr: atributos[k], pt: v});
      }
    }
  }

  function traduz(pt) {
    // O espaço em volta é preservado: ele separa palavras de nós vizinhos
    // ("é só o " + "<em>começo.</em>"), e comer o espaço cola as palavras.
    var esquerda = pt.match(/^\s*/)[0];
    var direita = pt.match(/\s*$/)[0];
    var miolo = pt.trim();
    var en = EN[miolo];
    return en === undefined ? pt : esquerda + en + direita;
  }

  function aplica() {
    for (var i = 0; i < nos.length; i++) {
      var item = nos[i];
      var valor = idioma === 'en' ? traduz(item.pt) : item.pt;
      if (item.no) item.no.nodeValue = valor;
      else if (item.el === document) document.title = valor;
      else item.el.setAttribute(item.attr, valor);
    }
    document.documentElement.lang = idioma === 'en' ? 'en' : 'pt-BR';
    var botao = document.getElementById('troca-idioma');
    if (botao) {
      // O botão mostra pra onde VAI, não onde está: é o que se entende sem
      // legenda nenhuma.
      botao.textContent = idioma === 'en' ? 'Português' : 'English';
      botao.setAttribute(
        'aria-label',
        idioma === 'en' ? 'Mudar para português' : 'Switch to English'
      );
    }
  }

  function poeBotao() {
    var nav = document.querySelector('header nav');
    if (!nav) return;
    var botao = document.createElement('button');
    botao.id = 'troca-idioma';
    botao.type = 'button';
    botao.className = 'lang-toggle';
    botao.addEventListener('click', function () {
      idioma = idioma === 'en' ? 'pt' : 'en';
      guarda(idioma);
      aplica();
    });
    nav.appendChild(botao);
  }

  function inicia() {
    coleta();
    poeBotao();
    aplica();
  }

  if (document.readyState === 'loading') {
    document.addEventListener('DOMContentLoaded', inicia);
  } else {
    inicia();
  }
})();
