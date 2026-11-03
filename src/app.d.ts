// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
	namespace App {
		// interface Error {}
		// interface Locals {}
		// interface PageData {}
		// interface PageState {}
		// interface Platform {}
	}
}

declare module "svelte-sonner" {
	import type { Component } from "svelte";

	export interface ExternalToast {
		id?: number | string;
		description?: string | Component<any>;
		duration?: number;
		dismissible?: boolean;
		action?: {
			label: string;
			onClick: (event: MouseEvent) => void;
		};
		cancel?: {
			label: string;
			onClick?: (event: MouseEvent) => void;
		};
		onDismiss?: (toast: any) => void;
		onAutoClose?: (toast: any) => void;
		position?: string;
		[key: string]: any;
	}

	export interface ToastFunction {
		(message: string | Component<any>, data?: ExternalToast): string | number;
		success(message: string | Component<any>, data?: ExternalToast): string | number;
		error(message: string | Component<any>, data?: ExternalToast): string | number;
		info(message: string | Component<any>, data?: ExternalToast): string | number;
		warning(message: string | Component<any>, data?: ExternalToast): string | number;
		loading(message: string | Component<any>, data?: ExternalToast): string | number;
		promise<T>(
			promise: Promise<T> | (() => Promise<T>),
			data?: {
				loading?: string | Component<any>;
				success?: string | Component<any> | ((data: T) => string | Component<any>);
				error?: string | Component<any> | ((error: any) => string | Component<any>);
				description?: string | Component<any>;
			}
		): string | number | undefined;
		dismiss(id?: number | string): string | number | undefined;
		remove(id?: number | string): string | number | undefined;
	}

	export const toast: ToastFunction;
	export const Toaster: Component<any>;
	export type ToasterProps = Record<string, any>;
}

declare module "*.mp3" {
	const src: string;
	export default src;
}

export {};
