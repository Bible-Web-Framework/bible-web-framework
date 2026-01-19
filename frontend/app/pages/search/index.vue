<script lang="ts" setup>
const route = useRoute();
const query = ref((route.query.q || '').toString());
const activeQuery = ref(query.value);
const { data: searchResults, pending, error } = await useFetch(() => `http://127.0.0.1:8080/v1/search?term=${activeQuery.value}`);

function search() {
  activeQuery.value = query.value;

  const url = new URL(window.location.href)
  url.searchParams.set('q', query.value);
  history.pushState(null, '', url);
}
</script>

<template>
    <div>
        <h1>Search Page</h1>
        <input v-model="query" placeholder="Enter search term" />
        <button @click="search">Search</button>

        <div v-if="pending">Loading...</div>
        <div v-else-if="error">Error: {{ error.message }}</div>
        <div v-else>
            <h2>Search Results:</h2>
            <table v-if="searchResults.response_type == 'search_results'">
                <tr v-for="result in searchResults.references" :key="JSON.stringify(result.reference)">
                    <td>{{ result.translated_book_name }} {{ result.reference.chapter }}:{{ result.reference.verses }}
                    </td>
                    <td v-for="content1 in result.content">
                        &lt;!&ndash; {{content1.content}} &ndash;&gt;
                        <span v-if="content1.marker == 'p' && content1.content[0].marker.contains('v', 'wj')">{{
                            content1.content[1]
                            }}</span>
                    </td>
                </tr>
            </table>
            <table v-if="searchResults.response_type == 'scripture_passages'">
                <tr v-for="result in searchResults.references" :key="JSON.stringify(result.reference)">
                    <td>{{ result.reference.book }} {{ result.reference.chapter }}:{{ result.reference.verses }}</td>
                    <td>{{ result.content[1] }}</td>
                </tr>
            </table>
            <ul>
                <li v-for="result in searchResults.references" :key="JSON.stringify(result.reference)">
                    {{ result.reference.book }} {{ result.reference.chapter }}:{{ result.reference.verses }}
                </li>
            </ul>
        </div>
    </div>
</template>
