<script setup lang="ts">
import { CheckIcon, CopyIcon, ExternalIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	injectAuth,
	injectNotificationManager,
	NewModal,
	SegmentedControl,
} from '@modrinth/ui'
import dayjs from 'dayjs'
import { computed, nextTick, ref, watch } from 'vue'
import { useRouter } from 'vue-router'

import { create_shared_instance_invite_link, get } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'

import { createShareCode, type ShareCodeResult } from './share-code'
import { shareInstanceId } from './store'

const router = useRouter()
const auth = injectAuth()
const { handleError } = injectNotificationManager()

const modal = ref<InstanceType<typeof NewModal> | null>(null)
function onHide() {
	shareInstanceId.value = null
}
const MODES = ['Code (ohne Account)', 'Kurzer Code (Modrinth)'] as const
const mode = ref<(typeof MODES)[number]>('Code (ohne Account)')

const instance = ref<GameInstance | null>(null)
const offline = ref<ShareCodeResult | null>(null)
const invite = ref<{ inviteId: string; expiresAt: string } | null>(null)
const working = ref(false)
const copied = ref(false)

const shared = computed(() => instance.value?.shared_instance ?? null)
const loggedIn = computed(() => !!auth.session_token.value)
const shownCode = computed(() =>
	mode.value === 'Code (ohne Account)' ? offline.value?.code : invite.value?.inviteId,
)

async function generateOffline() {
	if (!instance.value) return
	working.value = true
	try {
		offline.value = await createShareCode(instance.value.id)
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = false
	}
}

async function generateInvite() {
	if (!instance.value) return
	working.value = true
	try {
		invite.value = await create_shared_instance_invite_link(instance.value.id, {
			maxUses: 10,
			maxAgeSeconds: 7 * 24 * 60 * 60,
		})
	} catch (e) {
		handleError(e as Error)
	} finally {
		working.value = false
	}
}

async function copy() {
	if (!shownCode.value) return
	await navigator.clipboard.writeText(shownCode.value)
	copied.value = true
	setTimeout(() => (copied.value = false), 2000)
}

function openModrinthShare() {
	if (!instance.value) return
	const id = instance.value.id
	modal.value?.hide()
	router.push(`/instance/${encodeURIComponent(id)}/share`)
}

watch(shareInstanceId, async (id) => {
	if (!id) return
	offline.value = null
	invite.value = null
	copied.value = false
	mode.value = 'Code (ohne Account)'
	instance.value = await get(id)
	await nextTick()
	modal.value?.show()
	await generateOffline()
})

watch(mode, () => {
	copied.value = false
	if (
		mode.value === 'Kurzer Code (Modrinth)' &&
		!invite.value &&
		shared.value?.role === 'owner' &&
		loggedIn.value
	) {
		generateInvite()
	}
})
</script>

<template>
	<NewModal
		ref="modal"
		:header="instance ? `„${instance.name}“ teilen` : 'Profil teilen'"
		max-width="600px"
		width="600px"
		:on-hide="onHide"
	>
		<div class="flex flex-col gap-4">
			<SegmentedControl
				v-model="mode"
				:items="[...MODES]"
				aria-label="Art des Codes"
				full-width
				equal-width
			/>

			<!-- Offline code -->
			<template v-if="mode === 'Code (ohne Account)'">
				<p class="m-0 text-secondary">
					Schicke diesen Code deinen Freunden. In Modrinth Next klicken sie in der Bibliothek auf
					<b>„Mit Code importieren“</b> und bekommen dasselbe Profil. Hast du später Mods geändert,
					teile einfach erneut – der neue Code aktualisiert ihr Profil.
				</p>
				<div v-if="working && !offline" class="flex items-center gap-2 text-secondary">
					<SpinnerIcon class="animate-spin" /> Code wird erstellt…
				</div>
				<template v-if="offline">
					<textarea
						readonly
						:value="offline.code"
						rows="5"
						class="w-full resize-none break-all rounded-xl border border-solid border-surface-4 bg-surface-2 p-3 font-mono text-xs text-primary"
						@focus="($event.target as HTMLTextAreaElement).select()"
					/>
					<div class="text-sm text-secondary">
						{{ offline.payload.files.length }} Inhalte · {{ offline.payload.loader }}
						{{ offline.payload.game }} · {{ offline.code.length }} Zeichen
					</div>
					<Admonition v-if="offline.skipped > 0" type="warning">
						{{ offline.skipped }} Datei(en) sind nicht von Modrinth und können nicht im Code
						enthalten sein. Nutze dafür den kurzen Modrinth-Code.
					</Admonition>
				</template>
			</template>

			<!-- Modrinth shared instance -->
			<template v-else>
				<p class="m-0 text-secondary">
					Kurzer Code über Modrinths „Geteilte Profile“. Enthält auch Configs und eigene Dateien.
					Alle brauchen einen Modrinth-Account. Ein Code gilt 7 Tage und für bis zu 10 Personen.
				</p>
				<Admonition v-if="!loggedIn" type="info">
					Melde dich zuerst mit deinem Modrinth-Account an.
				</Admonition>
				<template v-else-if="!shared">
					<Admonition type="info">
						Dieses Profil ist noch nicht auf Modrinth geteilt. Veröffentliche es einmal – danach
						kannst du hier Codes erstellen und Updates veröffentlichen.
					</Admonition>
					<Button type="colored" color="brand" class="self-start" @click="openModrinthShare">
						<ExternalIcon /> Profil auf Modrinth teilen
					</Button>
				</template>
				<Admonition v-else-if="shared.role !== 'owner'" type="info">
					Nur die Person, die das Profil geteilt hat, kann Codes erstellen.
				</Admonition>
				<template v-else>
					<div v-if="working && !invite" class="flex items-center gap-2 text-secondary">
						<SpinnerIcon class="animate-spin" /> Code wird erstellt…
					</div>
					<div
						v-if="invite"
						class="rounded-2xl border border-solid border-surface-4 bg-surface-2 p-4 text-center"
					>
						<div class="font-mono text-2xl font-extrabold tracking-wider text-contrast">
							{{ invite.inviteId }}
						</div>
						<div class="mt-1 text-sm text-secondary">
							Gültig bis {{ dayjs(invite.expiresAt).format('DD.MM.YYYY HH:mm') }}
						</div>
					</div>
					<p class="m-0 text-sm text-secondary">
						Neue Version? Öffne die
						<button type="button" class="text-link hover:underline" @click="openModrinthShare">
							Teilen-Seite des Profils
						</button>
						und veröffentliche ein Update – alle Mitglieder bekommen es automatisch angeboten.
					</p>
				</template>
			</template>

			<div class="flex justify-end gap-2">
				<Button type="quiet" @click="modal?.hide()">Schließen</Button>
				<Button type="colored" color="brand" :disabled="!shownCode" @click="copy">
					<CheckIcon v-if="copied" />
					<CopyIcon v-else />
					{{ copied ? 'Kopiert!' : 'Code kopieren' }}
				</Button>
			</div>
		</div>
	</NewModal>
</template>
