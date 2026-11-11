package openidea.quarkus.overlay;

import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import org.eclipse.core.resources.IProject;
import org.eclipse.core.resources.ResourcesPlugin;
import org.eclipse.core.runtime.IProgressMonitor;
import org.eclipse.jdt.core.JavaCore;
import org.eclipse.jdt.ls.core.internal.IDelegateCommandHandler;
import org.eclipse.lsp4mp.jdt.core.project.IConfigSource;
import org.eclipse.lsp4mp.jdt.core.project.JDTMicroProfileProject;
import org.eclipse.lsp4mp.jdt.core.project.JDTMicroProfileProjectManager;
import org.eclipse.lsp4mp.jdt.core.project.PropertiesConfigSource;

/** Broker-only cache barrier. The broker must serialize this with its MP Java requests. */
public final class OverlayCommand implements IDelegateCommandHandler {
    public static final String COMMAND = "openidea.configOverlay.synchronize";
    private static final int MAX_BYTES = 8 * 1024 * 1024;
    private static final int MAX_DOCUMENTS = 32;
    private static final int MAX_TEXT = 512 * 1024;
    private static final Set<JDTMicroProfileProject> touched = new HashSet<>();
    private static long lastGeneration = -1;

    @Override
    public Object executeCommand(String command, List<Object> arguments, IProgressMonitor monitor) throws Exception {
        if (!COMMAND.equals(command)) throw new IllegalArgumentException("Unsupported overlay command");
        synchronized (OverlayCommand.class) {
            String file = System.getProperty("openidea.configOverlay");
            if (file == null || file.isBlank()) throw new IllegalStateException("Configuration overlay not enabled");
            Path snapshot = Path.of(file);
            if (!snapshot.isAbsolute() || !Files.isRegularFile(snapshot) || Files.isSymbolicLink(snapshot)
                    || Files.size(snapshot) > MAX_BYTES) throw new IllegalArgumentException("Invalid overlay snapshot");
            byte[] bytes = Files.readAllBytes(snapshot);
            if (bytes.length > MAX_BYTES) throw new IllegalArgumentException("Oversized overlay snapshot");
            JsonObject json = JsonParser.parseString(new String(bytes, StandardCharsets.UTF_8)).getAsJsonObject();
            if (json.get("version").getAsInt() != 1) throw new IllegalArgumentException("Unknown overlay schema");
            long generation = json.get("generation").getAsLong();
            if (generation < 0 || generation < lastGeneration) throw new IllegalArgumentException("Stale overlay snapshot");
            if (arguments == null || arguments.size() != 1 || !(arguments.get(0) instanceof Number)
                    || ((Number) arguments.get(0)).longValue() != generation)
                throw new IllegalArgumentException("Expected overlay generation required");
            Path root = localPath(json.get("rootUri").getAsString()).toRealPath();
            Map<Path, JsonObject> documents = new LinkedHashMap<>();
            if (json.getAsJsonArray("documents").size() > MAX_DOCUMENTS) throw new IllegalArgumentException("Too many overlay documents");
            for (var item : json.getAsJsonArray("documents")) {
                JsonObject document = item.getAsJsonObject();
                Path path = physicalPath(document.get("uri").getAsString());
                if (!path.startsWith(root) || !path.getFileName().toString().endsWith(".properties")
                        || document.get("text").getAsString().length() > MAX_TEXT
                        || document.get("version").getAsLong() < 0 || documents.put(path, document) != null)
                    throw new IllegalArgumentException("Invalid overlay document");
                // Existing paths must not escape the reviewed workspace through a symlink.
                if (Files.exists(path) && !path.toRealPath().startsWith(root))
                    throw new IllegalArgumentException("Overlay outside workspace");
            }
            // Always restore previously touched caches, including removed buffers and closed projects.
            for (var project : touched) project.evictConfigSourcesCache();
            touched.clear();
            Set<Path> applied = new HashSet<>();
            try {
                for (IProject project : ResourcesPlugin.getWorkspace().getRoot().getProjects()) {
                    if (monitor != null && monitor.isCanceled()) throw new IllegalStateException("Overlay canceled");
                    if (!project.isAccessible() || !project.hasNature(JavaCore.NATURE_ID)) continue;
                    URI location = project.getLocationURI();
                    if (location == null || !"file".equals(location.getScheme())) continue;
                    Path projectRoot = Path.of(location).toRealPath();
                    if (!projectRoot.startsWith(root) && !root.startsWith(projectRoot)) continue;
                    JDTMicroProfileProject mp = JDTMicroProfileProjectManager.getInstance()
                        .getJDTMicroProfileProject(JavaCore.create(project));
                    if (mp == null) continue;
                    mp.evictConfigSourcesCache();
                    touched.add(mp);
                    List<IConfigSource> sources = mp.getConfigSources();
                    for (int i = 0; i < sources.size(); i++) {
                        IConfigSource original = sources.get(i);
                        if (!(original instanceof PropertiesConfigSource) || original.getSourceConfigFileURI() == null) continue;
                        Path source;
                        try { source = physicalPath(original.getSourceConfigFileURI()); }
                        catch (java.io.IOException | RuntimeException ex) { continue; }
                        JsonObject document = documents.get(source);
                        if (document != null) {
                            sources.set(i, new OverlaySource(original, document.get("text").getAsString()));
                            applied.add(source);
                        }
                    }
                }
            } catch (Exception ex) {
                for (var project : touched) project.evictConfigSourcesCache();
                touched.clear();
                throw ex;
            }
            lastGeneration = generation;
            List<String> accepted = new ArrayList<>(), unsupported = new ArrayList<>();
            for (var entry : documents.entrySet())
                (applied.contains(entry.getKey()) ? accepted : unsupported).add(entry.getValue().get("uri").getAsString());
            return Map.of("generation", generation, "applied", accepted, "unsupported", unsupported);
        }
    }
    static Path physicalPath(String uri) throws java.io.IOException {
        Path path = localPath(uri);
        return Files.exists(path) ? path.toRealPath() : path;
    }
    static Path localPath(String uri) {
        URI parsed = URI.create(uri);
        if (!"file".equals(parsed.getScheme()) || parsed.getAuthority() != null || parsed.getQuery() != null
                || parsed.getFragment() != null) throw new IllegalArgumentException("Expected local file URI");
        return Path.of(parsed).toAbsolutePath().normalize();
    }
}
