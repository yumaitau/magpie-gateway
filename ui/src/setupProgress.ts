import { useSyncExternalStore } from 'react';

const key = 'magpie.setup.v1';
const event = 'magpie-setup-progress';
export function markSetupStep(step: 'testedModel' | 'connectedModel', model: string) {
	try {
		const current = JSON.parse(localStorage.getItem(key) || '{}');
		localStorage.setItem(key, JSON.stringify({ ...current, [step]: model }));
		window.dispatchEvent(new Event(event));
	} catch {
		/* Setup remains usable when browser storage is unavailable. */
	}
}
function subscribe(callback: () => void) {
	window.addEventListener(event, callback);
	window.addEventListener('storage', callback);
	return () => {
		window.removeEventListener(event, callback);
		window.removeEventListener('storage', callback);
	};
}
function snapshot() {
	try {
		return localStorage.getItem(key) || '{}';
	} catch {
		return '{}';
	}
}
export function useSetupProgress(): { testedModel?: string; connectedModel?: string } {
	const value = useSyncExternalStore(subscribe, snapshot, () => '{}');
	try {
		return JSON.parse(value) || {};
	} catch {
		return {};
	}
}
