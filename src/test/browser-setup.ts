// Applies the app's real CSS inside the browser test project so screenshots
// reflect production styling. The unit (jsdom) project uses ./setup.ts instead.
import '../ui/styles.css'
