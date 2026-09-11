(() => {
  "use strict";
  const $ = id => document.getElementById(id), buttons = [...document.querySelectorAll("[data-mood]")];
  const reduced = matchMedia("(prefers-reduced-motion: reduce)");
  function update(detail) {
    for (const button of buttons) button.setAttribute("aria-pressed", String(button.dataset.mood === detail.mood));
    for (const button of document.querySelectorAll("[data-gesture]")) button.dataset.active = String(button.dataset.gesture === detail.gesture);
    $("scene-name").textContent = detail.title; $("scene-description").textContent = detail.description;
    $("scene-number").textContent = String(buttons.findIndex(b => b.dataset.mood === detail.mood) + 1).padStart(2, "0") + " / 12";
    $("reel").setAttribute("aria-pressed", String(detail.showreel)); $("reel").textContent = detail.showreel ? "Pause showreel Ⅱ" : "Play showreel ▷";
    if (window.narrator && window.narrator.isPlaying) {
      const v = window.narrator.speechVoice === "vibevoice" ? `VibeVoice (${window.narrator.selectedPreset})` : window.narrator.speechVoice === "speechSynthesis" ? "Web Speech" : "Silent gestures";
      $("motion-status").textContent = `Phi is telling a fable · ${v}`;
    } else {
      $("motion-status").textContent = detail.reducedMotion ? "Reduced motion · still pose" : detail.animated ? "Spring motion · preview" : "Motion paused · preview";
    }
    $("scene-progress").style.opacity = detail.showreel ? 1 : .25;
  }
  const fox = PhiMotion.mount($("portrait"), { showreel: true, onChange: update, onFrame: ({ progress }) => { $("scene-progress").firstElementChild.style.transform = `scaleX(${progress})`; } });
  window.phi = fox;

  // Initialize Fables of Phi storyteller
  const narrator = window.PhiFables ? new window.PhiFables.FableNarrator(fox) : null;
  window.narrator = narrator;

  if (narrator && $("fable-select")) {
    const select = $("fable-select");
    narrator.fables.forEach((f, idx) => {
      const opt = document.createElement("option");
      opt.value = idx;
      opt.textContent = `${idx + 1}. ${f.title}`;
      select.appendChild(opt);
    });
    select.addEventListener("change", e => narrator.selectFable(Number(e.target.value)));
  }

  if (narrator && $("fable-play")) {
    $("fable-play").addEventListener("click", () => {
      if (narrator.isPlaying) {
        narrator.stop();
      } else {
        narrator.play();
      }
    });
  }

  if (narrator && $("fable-stop")) {
    $("fable-stop").addEventListener("click", () => narrator.stop());
  }

  if (narrator && $("fable-voice-type")) {
    $("fable-voice-type").addEventListener("change", e => {
      narrator.setVoice(e.target.value);
      if ($("fable-preset-wrap")) {
        $("fable-preset-wrap").style.display = e.target.value === "vibevoice" ? "block" : "none";
      }
    });
  }

  if (narrator && $("fable-voice-preset")) {
    $("fable-voice-preset").addEventListener("change", e => narrator.setPreset(e.target.value));
  }

  if (narrator) {
    narrator.onStanzaChange = data => {
      if ($("fable-theatre")) {
        $("fable-theatre").hidden = false;
      }
      if ($("fable-tag")) $("fable-tag").textContent = data.fable.title;
      if ($("fable-step")) $("fable-step").textContent = `${data.index + 1} / ${data.total}`;
      if ($("fable-quote")) $("fable-quote").textContent = `“${data.stanza.text}”`;
      if ($("fable-moral")) {
        $("fable-moral").textContent = data.index === data.total - 1 ? `✦ Moral: ${data.fable.moral}` : "";
      }
      if ($("motion-status")) {
        const v = narrator.speechVoice === "vibevoice" ? `VibeVoice (${narrator.selectedPreset})` : narrator.speechVoice === "speechSynthesis" ? "Web Speech" : "Silent gestures";
        $("motion-status").textContent = `Phi is telling a fable · ${v}`;
      }
    };

    narrator.onStateChange = state => {
      if ($("fable-play")) {
        $("fable-play").textContent = state.isPlaying ? "Pause fable Ⅱ" : "Tell fable ▷";
        $("fable-play").setAttribute("aria-pressed", String(state.isPlaying));
      }
      if ($("fable-stop")) {
        $("fable-stop").disabled = !state.isPlaying;
      }
      if (!state.isPlaying && $("fable-theatre")) {
        setTimeout(() => {
          if (!narrator.isPlaying && $("fable-theatre")) $("fable-theatre").hidden = true;
        }, 4000);
      }
    };
  }

  $("animation").checked = !reduced.matches;
  const syncReduced = () => { $("animation").disabled = reduced.matches; $("animation").checked = reduced.matches ? false : fox.enabled; };
  reduced.addEventListener("change", syncReduced); syncReduced(); fox.emit();

  for (const button of buttons) {
    button.addEventListener("click", () => {
      if (narrator) narrator.stop();
      fox.setState(button.dataset.mood);
    });
  }
  for (const button of document.querySelectorAll("[data-gesture]")) {
    button.addEventListener("click", () => {
      if (narrator) narrator.stop();
      fox.gesture(button.dataset.gesture);
    });
  }

  $("animation").addEventListener("change", e => fox.setAnimation(e.target.checked));
  $("attention").addEventListener("change", e => fox.setAttention(e.target.checked));
  $("tempo").addEventListener("input", e => { fox.setTempo(Number(e.target.value)); $("tempo-value").value = fox.tempo.toFixed(2) + "×"; });
  $("reel").addEventListener("click", () => {
    if (narrator) narrator.stop();
    fox.setShowreel(!fox.auto);
  });
  $("reset").addEventListener("click", () => {
    if (narrator) narrator.stop();
    fox.setState("greeting").setTempo(1).setAttention(true).setAnimation(true).setShowreel(true);
    $("attention").checked = true; $("animation").checked = !reduced.matches; $("tempo").value = 1; $("tempo-value").value = "1.00×";
  });
  $("theme").addEventListener("click", e => { const light = document.body.classList.toggle("paper"); e.target.textContent = light ? "Night ↗" : "Paper ↗"; e.target.setAttribute("aria-label", light ? "Switch to dark background" : "Switch to paper background"); });
  function save(source, name) {
    const url = URL.createObjectURL(new Blob([source], { type: "image/svg+xml" })), a = document.createElement("a");
    a.href = url; a.download = name; document.body.append(a); a.click(); a.remove(); setTimeout(() => URL.revokeObjectURL(url), 1500);
  }
  $("download-pose").addEventListener("click", () => save(fox.snapshot(), "selfware-phi-" + fox.mood + "-pose.svg"));
  $("download-loop").addEventListener("click", async () => {
    const button = $("download-loop"), old = button.innerHTML, mood = fox.mood;
    button.disabled = true; button.textContent = "Building the vector loop…";
    try { save(await fox.animatedSVG(), "selfware-phi-" + mood + "-animated.svg"); $("export-note").textContent = "Animated SVG saved. Includes a still pose for reduced motion."; }
    catch (error) { $("export-note").textContent = "Export did not complete: " + error.message; }
    finally { button.disabled = false; button.innerHTML = old; }
  });
})();
