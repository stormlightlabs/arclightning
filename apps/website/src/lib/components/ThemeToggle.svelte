<script lang="ts">
	import { Icon } from '@arclightning/ui';
	type Theme = 'light' | 'dark';
	const storageKey = 'arcl-theme';
	let isDark = $state(false);

	function readTheme(): Theme | null {
		try {
			const value = localStorage.getItem(storageKey);
			return value === 'light' || value === 'dark' ? value : null;
		} catch {
			return null;
		}
	}

	function applyTheme(theme: Theme): void {
		const root = document.documentElement;
		root.dataset.arclTheme = theme;
		// Read the canvas token after the theme attribute changes, so the browser color follows tokens.css.
		const canvas = getComputedStyle(root).getPropertyValue('--arcl-canvas').trim();
		if (canvas) document.querySelector('meta[name="theme-color"]')?.setAttribute('content', canvas);
		try {
			localStorage.setItem(storageKey, theme);
		} catch {
			/* The preference still applies for this page. */
		}
		isDark = theme === 'dark';
	}

	$effect(() => {
		const media = matchMedia('(prefers-color-scheme: dark)');
		isDark = (readTheme() ?? (media.matches ? 'dark' : 'light')) === 'dark';
		const followSystem = (): void => {
			if (!readTheme()) isDark = media.matches;
		};
		media.addEventListener('change', followSystem);
		return () => media.removeEventListener('change', followSystem);
	});
</script>

<button
	class="theme-toggle"
	type="button"
	aria-label={isDark ? 'Switch to light mode' : 'Switch to dark mode'}
	aria-pressed={isDark}
	onclick={() => applyTheme(isDark ? 'light' : 'dark')}>
	<Icon name={isDark ? 'sun' : 'moon'} size={17} /><span>{isDark ? 'Light' : 'Dark'}</span>
</button>
