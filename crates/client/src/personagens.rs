//! Selecao e criacao usam o mesmo contrato de personagens do servidor.
use macroquad::prelude::*;
use macroquad::material::{gl_use_default_material,gl_use_material,Material};
use shared::{Faction,VisualConfig,skills::Conjunto,protocol::{CharacterListEntry,ClientMessage}};
use crate::{hud_estilo as ui, render3d, vox::VoxCache};

pub enum Acao { Enviar(ClientMessage), Voltar }

#[derive(Default)]
pub struct Personagens {
    pub criando: bool,
    pub nome: String,
    pub arma: Option<u16>,
    pub faccao: Faction,
    pub mensagem: Option<String>,
    aguardando: Option<(String,f64)>,
    foco_nome: bool,
    scroll: usize,
    retrato: Option<RenderTarget>,
    giro: f32,
    mouse_anterior: Option<Vec2>,
    saida_previa: Option<RenderTarget>,
}

pub fn valida_nome(nome: &str) -> Result<&str,&'static str> {
    let nome=nome.trim();
    if !(2..=24).contains(&nome.len()) { return Err("Use de 2 a 24 caracteres."); }
    if !nome.chars().all(|c|c.is_ascii_alphanumeric() || c=='_') { return Err("Use letras sem acento, números ou _."); }
    Ok(nome)
}

fn conjunto_disponivel(id: u16, armas: &[u16]) -> bool {
    armas.contains(&id) && Conjunto::TODOS.iter().any(|c|c.arma()==id)
}

impl Personagens {
    pub fn recebeu_lista(&mut self, chars: &[CharacterListEntry], armas: &[u16], selecionado: &mut usize) {
        if let Some((nome,_))=self.aguardando.take() {
            if let Some(i)=chars.iter().position(|c|c.name==nome) {
                *selecionado=i;self.criando=false;self.mensagem=Some("Personagem criado. Sua jornada pode começar.".into());
            }
        } else if chars.is_empty() { self.criando=true;self.foco_nome=true; }
        *selecionado=(*selecionado).min(chars.len().saturating_sub(1));
        if !self.arma.is_some_and(|id|conjunto_disponivel(id,armas)) {
            self.arma=Conjunto::TODOS.iter().map(|c|c.arma()).find(|id|armas.contains(id));
        }
        self.scroll=selecionado.saturating_sub(3);
    }

    /// O campo de nome esta' com foco (o teclado da tela tem que estar aberto).
    pub fn foco_no_nome(&self) -> bool {
        self.criando && self.foco_nome && self.aguardando.is_none()
    }

    pub fn falhou(&mut self, motivo: String) {
        self.aguardando=None;self.criando=true;
        self.mensagem=Some(match motivo.as_str() {
            "nome ja em uso"=>"Esse nome já está em uso. Escolha outro.".into(),
            "nome 2-24 chars"=>"Use um nome de 2 a 24 caracteres.".into(),
            "nome so letras/numeros/_"=>"Use letras sem acento, números ou _.".into(),
            _=>motivo,
        });
    }

    fn pedido_criacao(&self, armas: &[u16]) -> Result<ClientMessage,&'static str> {
        let nome=valida_nome(&self.nome)?;
        let arma=self.arma.filter(|&id|conjunto_disponivel(id,armas)).ok_or("Escolha uma arma disponível.")?;
        Ok(ClientMessage::CreateCharacter{name:nome.into(),visual:VisualConfig::default(),starting_weapon:arma,faction:self.faccao})
    }

    pub fn desenha(&mut self, chars: &[CharacterListEntry], armas: &[u16], selecionado: &mut usize,
        digitado: &[char], vox: &VoxCache, solido: &Material) -> Option<Acao> {
        let w=screen_width();let h=screen_height();
        self.camera_ui();
        clear_background(Color::new(0.026,0.041,0.063,1.0));
        for i in 0..20 {
            let x=(i as f32*157.3+get_time() as f32*(3.0+(i%3) as f32))%w;
            let y=(i as f32*89.7)%h;
            draw_circle(x,y,1.0,Color::new(0.85,0.73,0.47,0.18));
        }
        ui::texto(32.0,35.0,"T E M P E S T",14,ui::OURO);
        ui::texto(32.0,77.0,if self.criando {"Crie sua história"} else {"Escolha seu personagem"},32,ui::TEXTO);
        ui::texto(32.0,103.0,if self.criando {"Uma arma. Um novo começo."} else {"Seu próximo capítulo está esperando."},15,ui::SUAVE);
        let ocupado=self.aguardando.is_some();
        if botao(Rect::new(w-150.0,30.0,118.0,34.0),if self.criando {"Cancelar"} else {"Voltar"},!ocupado,false)
            || (!ocupado && is_key_pressed(KeyCode::Escape)) {
            self.mensagem=None;self.foco_nome=false;
            if self.criando {self.criando=false;} else {return Some(Acao::Voltar);}
        }
        if self.aguardando.as_ref().is_some_and(|(_,t)|get_time()-t>15.0) {
            self.mensagem=Some("Aguardando confirmação do servidor…".into());
        }
        if self.criando {
            self.desenha_criacao(armas,digitado,vox,solido)
        } else {
            self.desenha_selecao(chars,selecionado,vox,solido)
        }
    }

    fn desenha_criacao(&mut self, armas: &[u16], digitado: &[char], vox: &VoxCache, solido: &Material) -> Option<Acao> {
        let w=screen_width();let h=screen_height();
        let pw=(w*0.47).clamp(370.0,510.0);
        let painel=Rect::new(w-pw-28.0,130.0,pw,h-214.0);
        let retrato=Rect::new(26.0,128.0,(painel.x-46.0).max(50.0),h-218.0);
        let conjunto=Conjunto::da_arma(self.arma.unwrap_or(0));
        self.desenha_retrato(retrato,conjunto,vox,solido);
        ui::texto_centro(retrato.x+retrato.w*0.5,retrato.y+retrato.h-35.0,conjunto.nome(),24,ui::TEXTO);
        ui::texto_centro(retrato.x+retrato.w*0.5,retrato.y+retrato.h-12.0,"Arraste o personagem para girar",13,ui::SUAVE);
        ui::painel(painel);
        let x=painel.x+18.0;let lw=painel.w-36.0;
        let compacto=h<860.0; let card_h=if compacto {67.0} else {80.0};
        let ocupado=self.aguardando.is_some();
        ui::texto(x,painel.y+28.0,"01  ARMA INICIAL",13,ui::OURO);
        for (i,c) in Conjunto::TODOS.iter().enumerate() {
            let r=Rect::new(x+(i%2) as f32*(lw+8.0)*0.5,painel.y+42.0+(i/2) as f32*(card_h+8.0),(lw-8.0)*0.5,card_h);
            let disponivel=armas.contains(&c.arma());let ativo=self.arma==Some(c.arma());
            let cor=ui::cor_skill(*c as u32*3+1);
            draw_rectangle(r.x,r.y,r.w,r.h,if ativo {Color::new(cor.r*0.11,cor.g*0.11,cor.b*0.11,1.0)} else {ui::FUNDO});
            draw_rectangle_lines(r.x,r.y,r.w,r.h,if ativo {2.0} else {1.0},if ativo {cor} else {ui::BORDA});
            if !crate::icones_ui::skill(*c as u32*3+1,vec2(r.x+25.0,r.y+r.h*0.5),34.0,if disponivel {1.0} else {0.35}) {
                ui::icone(*c as u32*3+1,vec2(r.x+25.0,r.y+r.h*0.5),18.0,if disponivel {cor} else {ui::SUAVE});
            }
            ui::texto_ajustado(c.nome(),r.x+49.0,r.y+r.h*0.5-2.0,r.w-56.0,15,if disponivel {ui::TEXTO} else {ui::SUAVE});
            ui::texto(r.x+49.0,r.y+r.h*0.5+17.0,if !disponivel {"Indisponível"} else {estilo(*c).0},11,ui::SUAVE);
            if !ocupado && disponivel && clicou(r) { self.arma=Some(c.arma());self.mensagem=None;self.giro=0.0; }
        }
        let conjunto=Conjunto::da_arma(self.arma.unwrap_or(0));
        let y=painel.y+42.0+2.0*(card_h+8.0)+16.0;
        texto_linhas(estilo(conjunto).1,x,y,lw,14,ui::SUAVE);
        let sy=y+54.0;
        let skills=shared::skills::playtest();
        for (i,s) in skills.iter().filter(|s|s.conjunto==conjunto).enumerate() {
            let sx=x+i as f32*lw/3.0;
            if !crate::icones_ui::skill(s.id,vec2(sx+14.0,sy),24.0,1.0) {
                ui::icone(s.id,vec2(sx+14.0,sy),11.0,ui::cor_skill(s.id));
            }
            ui::texto(sx+31.0,sy+4.0,&format!("Lv {}",s.nivel_necessario()),12,ui::OURO);
            ui::texto_ajustado(&s.nome,sx,sy+24.0,lw/3.0-8.0,12,ui::TEXTO);
        }
        let ny=sy+58.0;
        ui::texto(x,ny,"02  NOME",13,ui::OURO);
        let campo=Rect::new(x,ny+12.0,lw,42.0);
        if !ocupado && is_mouse_button_pressed(MouseButton::Left) {self.foco_nome=campo.contains(Vec2::from(mouse_position()));}
        if !ocupado && self.foco_nome {
            for &c in digitado {
                if c as u32==8 {self.nome.pop();} else if !c.is_control() && self.nome.chars().count()<24 {self.nome.push(c);}
            }
            if is_key_pressed(KeyCode::Backspace) && !digitado.contains(&'\u{8}') {self.nome.pop();}
        }
        draw_rectangle(campo.x,campo.y,campo.w,campo.h,ui::FUNDO);
        draw_rectangle_lines(campo.x,campo.y,campo.w,campo.h,1.0,if self.foco_nome {ui::OURO} else {ui::BORDA});
        let cursor=if self.foco_nome && (get_time()*2.0) as u32%2==0 {"|"} else {""};
        ui::texto(campo.x+12.0,campo.y+27.0,&if self.nome.is_empty() && !self.foco_nome {"Nome do personagem".into()} else {format!("{}{cursor}",self.nome)},18,ui::TEXTO);
        ui::texto(x,ny+72.0,"2–24 caracteres · letras, números e _",12,ui::SUAVE);
        let fy=ny+102.0;
        ui::texto(x,fy,"03  FACÇÃO",13,ui::OURO);
        for (i,(f,n)) in [(Faction::Peacemain,"Peacemain"),(Faction::Morganeers,"Morganeers")].iter().enumerate() {
            let r=Rect::new(x+i as f32*(lw+8.0)*0.5,fy+12.0,(lw-8.0)*0.5,34.0);
            if botao(r,n,!ocupado,self.faccao==*f) {self.faccao=*f;}
        }
        ui::texto(x,fy+66.0,if self.faccao==Faction::Peacemain {"Aventureiros e exploradores."} else {"Piratas em busca de saques."},13,ui::SUAVE);
        if painel.y+painel.h>fy+124.0 {
            texto_linhas("Sua arma define as skills iniciais. Você pode trocar de arma durante a aventura.",x,fy+108.0,lw,13,ui::SUAVE);
        }
        let erro=if self.nome.is_empty() {None} else {valida_nome(&self.nome).err()};
        let msg=self.mensagem.as_deref().or(erro).or(if self.arma.is_none() {Some("Nenhuma arma inicial disponível neste servidor.")} else {None});
        if let Some(msg)=msg { texto_linhas(msg,32.0,h-56.0,(w-pw-74.0).max(200.0),14,ui::OURO); }
        let pode=!ocupado && self.pedido_criacao(armas).is_ok();
        if botao(Rect::new(painel.x,h-66.0,painel.w,42.0),if ocupado {"Criando personagem…"} else {"Criar personagem"},pode,true)
            || (pode && self.foco_nome && is_key_pressed(KeyCode::Enter)) {
            let pedido=self.pedido_criacao(armas).ok()?;
            self.aguardando=Some((self.nome.trim().into(),get_time()));self.mensagem=None;
            return Some(Acao::Enviar(pedido));
        }
        None
    }

    fn desenha_selecao(&mut self, chars: &[CharacterListEntry], selecionado: &mut usize, vox: &VoxCache, solido: &Material) -> Option<Acao> {
        let w=screen_width();let h=screen_height();
        let r=Rect::new(28.0,136.0,(w*0.34).clamp(280.0,370.0),h-220.0);
        ui::painel(r);
        ui::texto(r.x+18.0,r.y+28.0,&format!("SEUS PERSONAGENS  ·  {}",chars.len()),13,ui::OURO);
        let visiveis=((r.h-112.0)/87.0).max(1.0) as usize;
        if r.contains(Vec2::from(mouse_position())) {
            let (_,roda)=mouse_wheel();
            if roda<0.0 {self.scroll=(self.scroll+1).min(chars.len().saturating_sub(visiveis));}
            if roda>0.0 {self.scroll=self.scroll.saturating_sub(1);}
        }
        if !chars.is_empty() {
            if is_key_pressed(KeyCode::Down) {*selecionado=(*selecionado+1).min(chars.len()-1);}
            if is_key_pressed(KeyCode::Up) {*selecionado=selecionado.saturating_sub(1);}
            if is_key_pressed(KeyCode::Down) || is_key_pressed(KeyCode::Up) {
                if *selecionado<self.scroll {self.scroll=*selecionado;}
                if *selecionado>=self.scroll+visiveis {self.scroll=*selecionado+1-visiveis;}
            }
        }
        for (i,c) in chars.iter().enumerate().skip(self.scroll).take(visiveis) {
            let card=Rect::new(r.x+12.0,r.y+45.0+(i-self.scroll) as f32*87.0,r.w-24.0,77.0);
            let conjunto=Conjunto::da_arma(c.weapon_id.unwrap_or(0));
            let cor=ui::cor_skill(conjunto as u32*3+1);
            draw_rectangle(card.x,card.y,card.w,card.h,if i==*selecionado {Color::new(0.10,0.13,0.17,1.0)} else {ui::FUNDO});
            draw_rectangle_lines(card.x,card.y,card.w,card.h,1.0,if i==*selecionado {ui::OURO} else {ui::BORDA});
            if !crate::icones_ui::skill(conjunto as u32*3+1,vec2(card.x+27.0,card.y+37.0),34.0,1.0) {
                ui::icone(conjunto as u32*3+1,vec2(card.x+27.0,card.y+37.0),18.0,cor);
            }
            ui::texto_ajustado(&c.name,card.x+54.0,card.y+29.0,card.w-62.0,19,ui::TEXTO);
            ui::texto(card.x+54.0,card.y+53.0,&format!("Lv {}  ·  {}",c.level,conjunto.nome()),12,ui::SUAVE);
            if clicou(card) {*selecionado=i;self.giro=0.0;self.mensagem=None;}
        }
        if chars.is_empty() {texto_linhas("Sua história começa aqui. Crie seu primeiro personagem.",r.x+18.0,r.y+85.0,r.w-36.0,18,ui::SUAVE);}
        if chars.len()>visiveis {ui::texto(r.x+18.0,r.y+r.h-62.0,"Role para ver mais personagens",12,ui::SUAVE);}
        if botao(Rect::new(r.x+12.0,r.y+r.h-50.0,r.w-24.0,38.0),"+  Novo personagem",true,false) {
            self.criando=true;self.nome.clear();self.mensagem=None;self.foco_nome=true;self.giro=0.0;
        }
        if let Some(c)=chars.get(*selecionado) {
            let conjunto=Conjunto::da_arma(c.weapon_id.unwrap_or(0));
            let hero=Rect::new(r.x+r.w+20.0,128.0,w-r.x-r.w-48.0,h-218.0);
            self.desenha_retrato(hero,conjunto,vox,solido);
            let cx=hero.x+hero.w*0.5;
            ui::texto_centro(cx,hero.y+hero.h-57.0,&c.name,30,ui::TEXTO);
            ui::texto_centro(cx,hero.y+hero.h-29.0,&format!("Nível {}  ·  {}",c.level,conjunto.nome()),16,ui::OURO);
            ui::texto_centro(cx,hero.y+hero.h-7.0,nome_faccao(c.faction),13,ui::SUAVE);
            if botao(Rect::new(hero.x,h-66.0,hero.w,42.0),"Entrar no mundo",true,true) || is_key_pressed(KeyCode::Enter) {
                return Some(Acao::Enviar(ClientMessage::SelectCharacter{name:c.name.clone()}));
            }
        }
        if let Some(msg)=&self.mensagem {texto_linhas(msg,32.0,h-58.0,r.w,14,ui::OURO);}
        None
    }

    fn camera_ui(&self) {
        if let Some(rt)=&self.saida_previa {
            let mut c=Camera2D::from_display_rect(Rect::new(0.0,0.0,screen_width(),screen_height()));
            c.render_target=Some(rt.clone());set_camera(&c);
        } else {set_default_camera();}
    }

    fn desenha_retrato(&mut self, r: Rect, conjunto: Conjunto, vox: &VoxCache, solido: &Material) {
        let w=r.w.max(1.0) as u32;let h=(r.h-84.0).max(1.0) as u32;
        if self.retrato.as_ref().is_none_or(|t|t.texture.width() as u32!=w || t.texture.height() as u32!=h) {
            let rt=render_target_ex(w,h,macroquad::texture::RenderTargetParams{sample_count:1,depth:true});
            rt.texture.set_filter(FilterMode::Linear);self.retrato=Some(rt);
        }
        let mouse=Vec2::from(mouse_position());
        if is_mouse_button_down(MouseButton::Left) && r.contains(mouse) {
            if let Some(antes)=self.mouse_anterior {self.giro+=(mouse.x-antes.x)*0.012;}
            self.mouse_anterior=Some(mouse);
        } else {self.mouse_anterior=None;}
        let Some(corpo)=vox.rig(render3d::RIG_CORPO) else {
            // Sem o corpo nao ha' o que girar — mas sumir com a area inteira
            // escondia a causa (no iPhone nenhum .vox carregava). Fundo, aviso
            // na tela e uma linha de log.
            static AVISOU: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
            if !AVISOU.swap(true, std::sync::atomic::Ordering::Relaxed) {
                eprintln!("[previa] rig {} nao carregou — veja as linhas [vox] falta", render3d::RIG_CORPO);
            }
            draw_rectangle(r.x,r.y,r.w,(r.h-84.0).max(1.0),Color::new(0.026,0.041,0.063,1.0));
            ui::texto_centro(r.x+r.w*0.5,r.y+(r.h-84.0)*0.5,"Modelo do personagem indisponível",14,ui::SUAVE);
            return;
        };
        let rt=self.retrato.as_ref().unwrap();
        let distancia=4.7*(0.62/(w as f32/h as f32)).max(1.0);
        let cam=Camera3D{position:vec3(0.0,1.35,distancia),target:vec3(0.0,0.88,0.0),up:Vec3::Y,
            fovy:34f32.to_radians(),aspect:Some(w as f32/h as f32),render_target:Some(rt.clone()),..Default::default()};
        set_camera(&cam);clear_background(Color::new(0.026,0.041,0.063,1.0));
        draw_cylinder(vec3(0.0,-0.07,0.0),0.90,0.93,0.06,None,Color::new(0.09,0.12,0.16,1.0));
        for i in 0..64 {let a=i as f32*std::f32::consts::TAU/64.0;let b=(i+1) as f32*std::f32::consts::TAU/64.0;
            draw_line_3d(vec3(a.cos()*0.88,-0.008,a.sin()*0.88),vec3(b.cos()*0.88,-0.008,b.sin()*0.88),ui::OURO);}
        gl_use_material(solido);solido.set_uniform("Recorte",Vec3::ZERO);
        let entrada=crate::rig::Entrada{fase:0.0,andar:0.0,correr:0.0,tempo:get_time() as f32,ar:0.0,degrau:[0.0,0.0],
            combate:crate::rig::Combate{conjunto:conjunto as u8,sacada:1.0,..Default::default()}};
        let pose=crate::rig::pose(&entrada);
        let base=Mat4::from_rotation_y(self.giro+0.18+(get_time() as f32*0.35).sin()*0.10);
        render3d::desenha_rig(base,&pose,corpo,vox.rig(render3d::RIG_CHAPEU),vox,None);
        gl_use_default_material();self.camera_ui();
        draw_texture_ex(&rt.texture,r.x,r.y,WHITE,DrawTextureParams{dest_size:Some(vec2(w as f32,h as f32)),flip_y:true,..Default::default()});
    }
}

fn nome_faccao(f: Faction) -> &'static str {match f {Faction::Peacemain=>"Peacemain",Faction::Morganeers=>"Morganeers"}}
fn estilo(c: Conjunto) -> (&'static str,&'static str) {
    match c {
        Conjunto::EspadaEscudo=>("Defesa e pressão","Lute na linha de frente com investidas, cortes amplos e uma barreira protetora."),
        Conjunto::Katana=>("Cortes e mobilidade","Empunhe com as duas mãos e combine saques rápidos, cortes em área e ondas de energia."),
        Conjunto::Pistolas=>("Combate à distância","Mantenha distância com tiros precisos, rajadas e um barril explosivo."),
        Conjunto::AnelMagico=>("Magia e suporte","Restaure sua vida, cure aliados próximos e invoque um impacto mágico sobre o alvo."),
    }
}
fn clicou(r: Rect) -> bool {is_mouse_button_pressed(MouseButton::Left) && r.contains(Vec2::from(mouse_position()))}
fn botao(r: Rect,t: &str,ativo: bool,destaque: bool) -> bool {
    let sobre=ativo && r.contains(Vec2::from(mouse_position()));
    draw_rectangle(r.x,r.y,r.w,r.h,if destaque && ativo {Color::new(0.24,0.21,0.14,1.0)} else {ui::FUNDO});
    draw_rectangle_lines(r.x,r.y,r.w,r.h,1.0,if ativo && (destaque || sobre) {ui::OURO} else {ui::BORDA});
    ui::texto_centro(r.x+r.w*0.5,r.y+r.h*0.5+6.0,t,16,if ativo {ui::TEXTO} else {ui::SUAVE});
    ativo && clicou(r)
}
fn texto_linhas(t: &str,x: f32,y: f32,w: f32,tam: u16,cor: Color) {
    let mut linha=String::new();let mut y=y;
    for palavra in t.split_whitespace() {
        let proxima=if linha.is_empty() {palavra.into()} else {format!("{linha} {palavra}")};
        if ui::medir(&proxima,tam)>w && !linha.is_empty() {ui::texto(x,y,&linha,tam,cor);y+=tam as f32+5.0;linha=palavra.into();}
        else {linha=proxima;}
    }
    ui::texto(x,y,&linha,tam,cor);
}

/// Previa sem login nem escritas no servidor. Exporta selecao e as quatro armas.
pub async fn previa(vox: &VoxCache) {
    let mut tela=Personagens::default();
    let chars=[("brunji",20,Conjunto::Katana),("Mare",5,Conjunto::Pistolas)]
        .map(|(n,level,c)|CharacterListEntry{name:n.into(),level,visual:VisualConfig::default(),weapon_id:Some(c.arma()),faction:Default::default()});
    let armas=Conjunto::TODOS.map(|c|c.arma());let mut sel=0;
    tela.recebeu_lista(&chars,&armas,&mut sel);
    let solido=render3d::material_solido();let inicio=get_time();let mut quadro=0;let mut cena=0;
    let exportar=std::env::var("MMO_PREVIA_EXPORTAR").is_ok();
    let mut teclado=crate::entrada::Teclado::novo();
    loop {
        teclado.coleta(get_time());
        if is_key_pressed(KeyCode::F10) {break;}
        if exportar {
            if tela.saida_previa.as_ref().is_none_or(|t|t.texture.width()!=screen_width() || t.texture.height()!=screen_height()) {
                tela.saida_previa=Some(render_target(screen_width() as u32,screen_height() as u32));
            }
            tela.criando=cena>0;
            if cena>0 {tela.arma=Some(armas[cena-1]);tela.nome="NovoHeroi".into();}
        }
        let _=tela.desenha(&chars,&armas,&mut sel,teclado.digitado(),vox,&solido);
        if exportar && get_time()-inicio>2.0 {
            quadro+=1;
            if quadro%6==0 {
                unsafe {get_internal_gl().flush();}
                tela.saida_previa.as_ref().unwrap().texture.get_texture_data().export_png(&format!("/tmp/2dengine-personagem-{cena}.png"));
                cena+=1;if cena==5 {break;}
            }
        }
        next_frame().await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nome_segue_validacao_do_servidor() {
        assert_eq!(valida_nome("  Ana_23 "),Ok("Ana_23"));
        for n in ["a","","nome com espaco","João","aaaaaaaaaaaaaaaaaaaaaaaaa"] {assert!(valida_nome(n).is_err());}
    }
    #[test]
    fn criacao_envia_arma_e_faccao_e_recusa_arma_indisponivel() {
        let mut p=Personagens::default();p.nome="Teste".into();p.arma=Some(Conjunto::Katana.arma());p.faccao=Faction::Morganeers;
        let armas=Conjunto::TODOS.map(|c|c.arma());
        match p.pedido_criacao(&armas).unwrap() {ClientMessage::CreateCharacter{name,starting_weapon,faction,..}=>{
            assert_eq!(name,"Teste");assert_eq!(starting_weapon,Conjunto::Katana.arma());assert_eq!(faction,Faction::Morganeers);
        },_=>panic!("mensagem incorreta")}
        assert!(p.pedido_criacao(&[Conjunto::Pistolas.arma()]).is_err());
    }
    #[test]
    fn confirmacao_seleciona_novo_personagem_e_erro_preserva_escolhas() {
        let mut p=Personagens::default();let mut sel=0;let armas=[Conjunto::Katana.arma()];
        p.recebeu_lista(&[],&armas,&mut sel);assert!(p.criando);assert_eq!(p.arma,Some(armas[0]));
        p.nome="Novo".into();p.aguardando=Some(("Novo".into(),0.0));
        let chars=["Antigo","Novo"].map(|n|CharacterListEntry{name:n.into(),level:1,weapon_id:Some(armas[0]),visual:VisualConfig::default(),faction:Default::default()});
        p.recebeu_lista(&chars,&armas,&mut sel);assert_eq!(sel,1);assert!(!p.criando);assert!(p.aguardando.is_none());
        p.falhou("nome ja em uso".into());assert_eq!(p.nome,"Novo");assert_eq!(p.arma,Some(armas[0]));assert!(p.criando);
    }
}
