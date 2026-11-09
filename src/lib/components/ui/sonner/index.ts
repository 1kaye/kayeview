import { toast as originalToast } from "svelte-sonner";
import popSound from "../../../../assets/sound_effect/pop_se.mp3";
import failedSound from "../../../../assets/sound_effect/failed_se.mp3";

let audioCtx: AudioContext | null = null;
const bufCache = new Map<string, AudioBuffer>();

async function playSound(src: string) {
	try {
		audioCtx ??= new AudioContext();
		if (audioCtx.state === "suspended") await audioCtx.resume();
		let buf = bufCache.get(src);
		if (!buf) {
			const res = await fetch(src);
			buf = await audioCtx.decodeAudioData(await res.arrayBuffer());
			bufCache.set(src, buf);
		}
		const source = audioCtx.createBufferSource();
		source.buffer = buf;
		source.connect(audioCtx.destination);
		source.start(0);
	} catch {
		// Abaikan jika pemutaran audio gagal atau diblokir browser
	}
}

const toast = new Proxy(originalToast, {
	get(target, prop, receiver) {
		if (prop === "success") {
			return (...args: Parameters<typeof originalToast.success>) => {
				playSound(popSound);
				return target.success(...args);
			};
		}
		if (prop === "error") {
			return (...args: Parameters<typeof originalToast.error>) => {
				playSound(failedSound);
				return target.error(...args);
			};
		}
		const val = Reflect.get(target, prop, receiver);
		return typeof val === "function" ? val.bind(target) : val;
	},
});

export { default as Toaster } from "./sonner.svelte";
export { toast };
