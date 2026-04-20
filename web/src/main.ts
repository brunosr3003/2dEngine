import { registerAccount, loginAccount, type ApiError } from "./api";

type TabName = "register" | "login";

function initTabs(): void {
  const tabs = document.querySelectorAll<HTMLButtonElement>(".tab");
  const cards: Record<TabName, HTMLFormElement> = {
    register: document.getElementById("form-register") as HTMLFormElement,
    login: document.getElementById("form-login") as HTMLFormElement,
  };

  tabs.forEach((btn) => {
    btn.addEventListener("click", () => {
      const tab = btn.dataset.tab as TabName;
      tabs.forEach((b) => b.classList.toggle("active", b === btn));
      (Object.keys(cards) as TabName[]).forEach((k) => {
        cards[k].classList.toggle("active", k === tab);
      });
    });
  });
}

function setMsg(form: HTMLFormElement, text: string, kind: "ok" | "error" | ""): void {
  const el = form.querySelector<HTMLParagraphElement>("[data-msg]");
  if (!el) return;
  el.textContent = text;
  el.classList.remove("ok", "error");
  if (kind) el.classList.add(kind);
}

function extractFormData<T extends Record<string, string>>(form: HTMLFormElement): T {
  const fd = new FormData(form);
  const out: Record<string, string> = {};
  fd.forEach((v, k) => {
    out[k] = typeof v === "string" ? v : "";
  });
  return out as T;
}

async function handleSubmit(
  form: HTMLFormElement,
  fn: (data: any) => Promise<{ username: string }>,
  successMsg: string,
): Promise<void> {
  const submit = form.querySelector<HTMLButtonElement>("button[type=submit]");
  if (!submit) return;

  submit.disabled = true;
  setMsg(form, "enviando…", "");

  try {
    const data = extractFormData(form);
    const res = await fn(data);
    setMsg(form, `${successMsg} (${res.username})`, "ok");
    form.reset();
  } catch (e) {
    const err = e as ApiError;
    setMsg(form, err.message || "erro desconhecido", "error");
  } finally {
    submit.disabled = false;
  }
}

function bindForms(): void {
  const regForm = document.getElementById("form-register") as HTMLFormElement;
  regForm.addEventListener("submit", (ev) => {
    ev.preventDefault();
    void handleSubmit(regForm, registerAccount, "conta criada");
  });

  const loginForm = document.getElementById("form-login") as HTMLFormElement;
  loginForm.addEventListener("submit", (ev) => {
    ev.preventDefault();
    void handleSubmit(loginForm, loginAccount, "login ok");
  });
}

initTabs();
bindForms();
