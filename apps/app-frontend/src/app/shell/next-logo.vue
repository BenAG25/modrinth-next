<script setup lang="ts">
// Modrinth Next logo. Replaces the Modrinth brand logo, which forks may not use (see COPYING.md).
withDefaults(defineProps<{ withText?: boolean }>(), { withText: true })
</script>

<template>
	<span class="inline-flex items-center gap-2">
		<svg viewBox="0 0 1024 1024" class="h-full w-auto" aria-hidden="true">
			<defs>
				<linearGradient id="modrinth-next-logo-bg" x1="0" y1="0" x2="1" y2="1">
					<stop offset="0" stop-color="#2563eb" />
					<stop offset="1" stop-color="#7c3aed" />
				</linearGradient>
			</defs>
			<rect x="48" y="48" width="928" height="928" rx="220" fill="url(#modrinth-next-logo-bg)" />
			<g fill="#ffffff" fill-opacity="0.18">
				<rect x="212" y="600" width="600" height="110" rx="34" />
				<rect x="212" y="740" width="600" height="110" rx="34" />
			</g>
			<g fill="#ffffff">
				<circle cx="282" cy="655" r="20" />
				<circle cx="282" cy="795" r="20" />
			</g>
			<path fill="#ffffff" d="M262 520V170h96l214 222V170h112v350h-96L374 298v222z" />
			<path fill="#ffffff" d="M720 230l112 115-112 115z" />
		</svg>
		<span v-if="withText" class="whitespace-nowrap text-lg font-extrabold leading-none">
			Modrinth Next
		</span>
	</span>
</template>
