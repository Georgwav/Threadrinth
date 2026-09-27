<script setup lang="ts">
import { CheckIcon, DownloadIcon, SearchIcon, SpinnerIcon, TrashIcon } from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	defineMessages,
	IconButton,
	injectNotificationManager,
	Input,
	Toggle,
	useCompactNumber,
	useVIntl,
} from '@modrinth/ui'
import { computed, onMounted, ref, watch } from 'vue'

import {
	type HostedServer,
	install_server_project,
	remove_server_content,
	search_server_content,
	server_content,
	type ServerContent,
	type ServerSearchHit,
	set_server_content_enabled,
} from '@/helpers/hosting'

/** A server's mods or plugins, with Modrinth search and one-click picks. */

const props = defineProps<{
	server: HostedServer
	running: boolean
}>()

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const { formatCompactNumber } = useCompactNumber()

const messages = defineMessages({
	installedMods: { id: 'app.hosting.content.installed-mods', defaultMessage: 'Installed mods' },
	installedPlugins: {
		id: 'app.hosting.content.installed-plugins',
		defaultMessage: 'Installed plugins',
	},
	none: { id: 'app.hosting.content.none', defaultMessage: 'Nothing installed yet.' },
	quickAdd: { id: 'app.hosting.content.quick-add', defaultMessage: 'Quick add' },
	quickAddHint: {
		id: 'app.hosting.content.quick-add-hint',
		defaultMessage: 'Popular performance and admin picks for this server.',
	},
	searchMods: { id: 'app.hosting.content.search-mods', defaultMessage: 'Search server mods…' },
	searchPlugins: { id: 'app.hosting.content.search-plugins', defaultMessage: 'Search plugins…' },
	install: { id: 'app.hosting.content.install', defaultMessage: 'Install' },
	installed: { id: 'app.hosting.content.installed', defaultMessage: 'Installed' },
	loadMore: { id: 'app.hosting.content.load-more', defaultMessage: 'Load more' },
	remove: { id: 'app.hosting.content.remove', defaultMessage: 'Remove' },
	added: { id: 'app.hosting.content.added', defaultMessage: 'Added {name}' },
	addedFiles: {
		id: 'app.hosting.content.added-files',
		defaultMessage: '{files}',
	},
	missing: {
		id: 'app.hosting.content.missing',
		defaultMessage: 'No version for this server of: {names}',
	},
	unmarked: {
		id: 'app.hosting.content.unmarked',
		defaultMessage:
			'{names} isn’t marked for Minecraft {version} yet, so its newest build was added. Most plugins keep working on newer versions.',
	},
	restart: {
		id: 'app.hosting.content.restart',
		defaultMessage: 'Restart the server to load changes.',
	},
	vanilla: {
		id: 'app.hosting.content.vanilla',
		defaultMessage:
			'Vanilla servers can’t load mods or plugins. Create a new server with a mod loader, Paper or Purpur to add them.',
	},
	downloads: { id: 'app.hosting.content.downloads', defaultMessage: '{count} downloads' },
})

type Pick = { slug: string; label: string }

const plugins = computed(() => props.server.platform !== null)
const supported = computed(() => plugins.value || props.server.loader !== 'vanilla')

const picks = computed<Pick[]>(() => {
	if (plugins.value) {
		return [
			{ slug: 'chunky', label: 'Chunky' },
			{ slug: 'luckperms', label: 'LuckPerms' },
			{ slug: 'essentialsx', label: 'EssentialsX' },
			{ slug: 'viaversion', label: 'ViaVersion' },
			{ slug: 'worldedit', label: 'WorldEdit' },
			{ slug: 'geyser', label: 'Geyser' },
		]
	}
	if (props.server.loader === 'fabric' || props.server.loader === 'quilt') {
		return [
			{ slug: 'lithium', label: 'Lithium' },
			{ slug: 'ferrite-core', label: 'FerriteCore' },
			{ slug: 'krypton', label: 'Krypton' },
			{ slug: 'c2me-fabric', label: 'C2ME' },
			{ slug: 'modernfix', label: 'ModernFix' },
			{ slug: 'spark', label: 'spark' },
			{ slug: 'chunky', label: 'Chunky' },
		]
	}
	return [
		{ slug: 'modernfix', label: 'ModernFix' },
		{ slug: 'ferrite-core', label: 'FerriteCore' },
		{ slug: 'spark', label: 'spark' },
		{ slug: 'chunky', label: 'Chunky' },
	]
})

const content = ref<ServerContent[]>([])
const loadingContent = ref(false)
const query = ref<string | number | undefined>('')
const hits = ref<ServerSearchHit[]>([])
const total = ref(0)
const searching = ref(false)
const installing = ref(new Set<string>())
const changed = ref(false)

const installedIds = computed(
	() => new Set(content.value.map((item) => item.project_id).filter(Boolean)),
)
const installedSlugs = computed(
	() =>
		new Set(
			hits.value.filter((hit) => installedIds.value.has(hit.project_id)).map((hit) => hit.slug),
		),
)

async function loadContent() {
	if (!supported.value) return
	loadingContent.value = true
	try {
		content.value = await server_content(props.server.id)
	} catch (error) {
		handleError(error as Error)
	} finally {
		loadingContent.value = false
	}
}

let searchId = 0
async function search(reset = true) {
	if (!supported.value) return
	const id = ++searchId
	searching.value = true
	try {
		const results = await search_server_content(
			props.server.id,
			String(query.value ?? ''),
			reset ? 0 : hits.value.length,
		)
		if (id !== searchId) return
		hits.value = reset ? results.hits : [...hits.value, ...results.hits]
		total.value = results.total
	} catch (error) {
		if (id === searchId) handleError(error as Error)
	} finally {
		if (id === searchId) searching.value = false
	}
}

let debounce: ReturnType<typeof setTimeout> | undefined
watch(query, () => {
	clearTimeout(debounce)
	debounce = setTimeout(() => void search(true), 350)
})

async function install(project: string, name: string) {
	installing.value = new Set([...installing.value, project])
	try {
		const result = await install_server_project(props.server.id, project)
		changed.value = true
		addNotification({
			title: formatMessage(messages.added, { name }),
			text: formatMessage(messages.addedFiles, { files: result.installed.join(', ') }),
			type: 'success',
		})
		if (result.unmarked.length > 0) {
			addNotification({
				title: formatMessage(messages.unmarked, {
					names: result.unmarked.join(', '),
					version: props.server.game_version,
				}),
				type: 'warning',
			})
		}
		if (result.missing_dependencies.length > 0) {
			addNotification({
				title: formatMessage(messages.missing, {
					names: result.missing_dependencies.join(', '),
				}),
				type: 'warning',
			})
		}
		await loadContent()
	} catch (error) {
		handleError(error as Error)
	} finally {
		const next = new Set(installing.value)
		next.delete(project)
		installing.value = next
	}
}

async function toggle(item: ServerContent, enabled: boolean) {
	try {
		await set_server_content_enabled(props.server.id, item.file_name, enabled)
		changed.value = true
		await loadContent()
	} catch (error) {
		handleError(error as Error)
	}
}

async function remove(item: ServerContent) {
	try {
		await remove_server_content(props.server.id, item.file_name)
		changed.value = true
		await loadContent()
	} catch (error) {
		handleError(error as Error)
	}
}

function pickInstalled(pick: Pick) {
	return (
		installedSlugs.value.has(pick.slug) ||
		content.value.some((item) => item.file_name.toLowerCase().startsWith(pick.slug))
	)
}

onMounted(() => {
	void loadContent()
	void search(true)
})
</script>

<template>
	<Admonition v-if="!supported" type="info" :header="formatMessage(messages.vanilla)" />
	<div v-else class="flex flex-col gap-4">
		<Admonition v-if="running && changed" type="info" :header="formatMessage(messages.restart)" />

		<div class="flex flex-col gap-2 rounded-2xl bg-bg-raised p-4">
			<span class="font-semibold text-contrast">{{ formatMessage(messages.quickAdd) }}</span>
			<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.quickAddHint) }}</p>
			<div class="flex flex-wrap gap-2">
				<Button
					v-for="pick in picks"
					:key="pick.slug"
					:disabled="installing.has(pick.slug) || pickInstalled(pick)"
					@click="install(pick.slug, pick.label)"
				>
					<SpinnerIcon v-if="installing.has(pick.slug)" class="animate-spin" />
					<CheckIcon v-else-if="pickInstalled(pick)" />
					<DownloadIcon v-else />
					{{ pick.label }}
				</Button>
			</div>
		</div>

		<div class="flex flex-col gap-2">
			<span class="font-semibold text-contrast">
				{{ formatMessage(plugins ? messages.installedPlugins : messages.installedMods) }}
				({{ content.length }})
			</span>
			<p v-if="!loadingContent && content.length === 0" class="m-0 text-secondary">
				{{ formatMessage(messages.none) }}
			</p>
			<div
				v-for="item in content"
				:key="item.file_name"
				class="flex items-center gap-3 rounded-2xl bg-bg-raised p-3"
			>
				<Avatar :src="item.icon_url" :alt="item.title ?? item.file_name" size="40px" />
				<div class="flex min-w-0 flex-1 flex-col">
					<span
						class="truncate font-semibold"
						:class="item.enabled ? 'text-contrast' : 'text-secondary'"
					>
						{{ item.title ?? item.file_name }}
					</span>
					<span class="truncate text-sm text-secondary">
						{{ item.version_number ?? item.file_name }}
					</span>
				</div>
				<Toggle
					:model-value="item.enabled"
					@update:model-value="(value?: boolean) => toggle(item, !!value)"
				/>
				<IconButton
					v-tooltip="formatMessage(messages.remove)"
					:label="formatMessage(messages.remove)"
					type="quiet"
					color="red"
					@click="remove(item)"
				>
					<TrashIcon />
				</IconButton>
			</div>
		</div>

		<div class="flex flex-col gap-2">
			<Input
				v-model="query"
				:icon="SearchIcon"
				type="text"
				:placeholder="formatMessage(plugins ? messages.searchPlugins : messages.searchMods)"
				clearable
				wrapper-class="w-full"
			/>
			<div
				v-for="hit in hits"
				:key="hit.project_id"
				class="flex items-center gap-3 rounded-2xl bg-bg-raised p-3"
			>
				<Avatar :src="hit.icon_url" :alt="hit.title" size="48px" />
				<div class="flex min-w-0 flex-1 flex-col gap-0.5">
					<span class="truncate font-semibold text-contrast">
						{{ hit.title }}
						<span class="text-sm font-normal text-secondary">· {{ hit.author }}</span>
					</span>
					<span class="line-clamp-2 text-sm text-secondary">{{ hit.description }}</span>
					<span class="text-xs text-secondary">
						{{ formatMessage(messages.downloads, { count: formatCompactNumber(hit.downloads) }) }}
					</span>
				</div>
				<Button
					:type="installedIds.has(hit.project_id) ? 'base' : 'colored'"
					:color="installedIds.has(hit.project_id) ? undefined : 'brand'"
					:disabled="installing.has(hit.project_id) || installedIds.has(hit.project_id)"
					@click="install(hit.project_id, hit.title)"
				>
					<SpinnerIcon v-if="installing.has(hit.project_id)" class="animate-spin" />
					<CheckIcon v-else-if="installedIds.has(hit.project_id)" />
					<DownloadIcon v-else />
					{{
						formatMessage(installedIds.has(hit.project_id) ? messages.installed : messages.install)
					}}
				</Button>
			</div>
			<div v-if="hits.length < total" class="flex justify-center">
				<Button :disabled="searching" @click="search(false)">
					{{ formatMessage(messages.loadMore) }}
				</Button>
			</div>
		</div>
	</div>
</template>
