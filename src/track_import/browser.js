void (async () => {
    const spotify = location.hostname === "open.spotify.com";
    const vk = ["vk.com", "www.vk.com", "m.vk.com", "vk.ru", "m.vk.ru", "music.vk.com"].includes(location.hostname);
    if (!spotify && !vk) {
        alert("Откройте список треков в Spotify или VK Музыке и нажмите закладку WeDex.");
        return;
    }

    const rowSelector = spotify
        ? '[data-testid="tracklist-row"]'
        : '.audio_row, [data-audio], [class~="AudioRow"], [class*="AudioRow_audioRow"]';
    const text = node => node?.textContent?.trim() || "";
    const read = row => {
        if (spotify) {
            const title = text(row.querySelector('a[href*="/track/"]'));
            const artists = [...row.querySelectorAll('a[href*="/artist/"]')].map(text).filter(Boolean);
            return {title, artist: [...new Set(artists)].join(", ")};
        }
        try {
            const audio = JSON.parse(row.getAttribute("data-audio"));
            if (typeof audio?.[3] === "string" && typeof audio?.[4] === "string") {
                return {title: audio[3], artist: audio[4]};
            }
        } catch {}
        const title = text(row.querySelector('.audio_row__title_inner, .audio_row__title a, [class*="AudioRow_title"]'));
        const artist = text(row.querySelector('.audio_row__performers, [class*="AudioRow_artist"], [class*="AudioRow_subtitle"]'));
        return {title, artist};
    };

    const initial = document.querySelector(rowSelector);
    if (!initial) {
        alert("Сначала откройте плейлист или аудиозаписи и дождитесь появления треков.");
        return;
    }
    if (document.querySelector("#wedex-track-export")) return;
    const dialog = document.createElement("dialog");
    dialog.id = "wedex-track-export";
    dialog.setAttribute("aria-label", "Перенос треков в WeDex");
    const status = document.createElement("p");
    const close = document.createElement("button");
    close.textContent = "Закрыть";
    close.onclick = () => dialog.close();
    dialog.addEventListener("close", () => dialog.remove());
    dialog.append(status, close);
    dialog.style.cssText = "position:fixed;inset:auto 16px 16px auto;width:min(380px,90vw);z-index:2147483647;padding:18px;border-radius:12px;background:white;color:black;";
    document.body.append(dialog);
    dialog.show();

    let scroller = initial.parentElement;
    while (scroller && !(scroller.scrollHeight > scroller.clientHeight + 20
            && /auto|scroll/.test(getComputedStyle(scroller).overflowY))) {
        scroller = scroller.parentElement;
    }
    scroller ||= document.scrollingElement;
    const now = scroller.scrollTop;
    const wait = ms => new Promise(resolve => setTimeout(resolve, ms));
    const tracks = new Map();
    let settled = 0;
    scroller.scrollTop = 0;
    await wait(300);
    for (let step = 0; dialog.open && step < 1600 && tracks.size < 10000; step++) {
        for (const row of document.querySelectorAll(rowSelector)) {
            const {title, artist} = read(row);
            if (title && artist) tracks.set(JSON.stringify([title, artist]), {title, artist});
        }
        status.textContent = `Читаем список · ${tracks.size}`;
        const position = scroller.scrollTop;
        scroller.scrollTop = position + Math.max(100, scroller.clientHeight * 0.75);
        settled = scroller.scrollTop === position ? settled + 1 : 0;
        if (settled >= 5) break;
        await wait(settled ? 420 : 160);
    }
    scroller.scrollTop = now;
    if (!dialog.open) return;
    if (!tracks.size) {
        status.textContent = "Треки на этой странице не распознаны.";
        return;
    }
    status.textContent = `Треков собрано: ${tracks.size}. Скопируйте список и вставьте в WeDex.`;
    const content = JSON.stringify({wedexTracks: [...tracks.values()]});
    const copy = document.createElement("button");
    copy.textContent = "Скопировать список";
    copy.onclick = async () => {
        try {
            await navigator.clipboard.writeText(content);
            dialog.close();
        } catch {
            const area = document.createElement("textarea");
            area.value = content;
            area.style.cssText = "width:100%;height:90px;";
            dialog.insertBefore(area, close);
            area.select();
            status.textContent = "Скопируйте выделенный список.";
            copy.remove();
        }
    };
    dialog.insertBefore(copy, close);
})();
