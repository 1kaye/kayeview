import { toast as originalToast } from "svelte-sonner";
import popSound from "../../../../assets/sound_effect/pop_se.mp3";
import failedSound from "../../../../assets/sound_effect/failed_se.mp3";

function playSound(src: string) {
	try {
		const audio = new Audio(src);
		audio.play().catch(() => {});
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
