/**
 * The safety advisories, for the people whose results they change.
 *
 * Source of truth: docs/advisories/ (one file per advisory, with the full
 * reproduction). This post is the reader's summary of those files and links
 * to each one; it says nothing they do not. Every number in the "documented
 * cases" table is quoted from the advisory named in its row, and the content
 * test pins them, so a figure cannot drift here without the advisory changing
 * first.
 *
 * Status is as of the post's date. An advisory's status changes when its fix
 * is DEPLOYED to stabileo.com (main auto-deploys), not when its pull request
 * is opened; update the row and the "Estado" item together when one lands.
 */
import type { Post } from '../types';

const REPO = 'https://github.com/lambdaclass/stabileo/blob/main/docs/advisories';

export const safetyAdvisories: Post = {
  slug: 'safety-advisories',
  date: '2026-10-20',
  order: 6,
  authors: ['Federico Carrone'],
  tagKeys: ['blog.tag.safety', 'blog.tag.verification'],
  i18n: {
    es: {
      title: 'Avisos de seguridad: qué resultados cambiaron y qué hacer',
      excerpt:
        'Los errores de Stabileo que pudieron dar resultados equivocados o del lado inseguro: a quién afectan, cómo saber si un modelo está afectado, qué hacer mientras tanto y cuándo quedaron corregidos.',
      blocks: [
        { k: 'p', t: 'Los resultados de Stabileo se usan para dimensionar estructuras reales. Cuando encontramos un error que pudo dar un resultado equivocado, y en particular uno del lado inseguro, lo publicamos como aviso de seguridad: qué estaba mal, qué modelos afecta, cómo reconocerlos, qué hacer hasta que la corrección esté publicada y qué revisar en los resultados que ya se usaron.' },
        { k: 'p', t: 'Casi todos estos errores los encontró nuestro propio programa de verificación: comparación contra otros siete programas de elementos finitos, ejemplos resueltos publicados, pruebas de invariantes físicos y revisión de las derivaciones de cada elemento. Publicarlos es parte de ese trabajo: un error que no se informa sigue en los cálculos que ya se entregaron.' },
        { k: 'note', t: 'Estado al 21 de octubre de 2026. Un aviso cuenta como corregido cuando la corrección está publicada en stabileo.com, no cuando existe el cambio. Mientras un aviso esté abierto, usá la alternativa que indica y revisá a mano el resultado afectado antes de firmar.' },

        { k: 'h', t: 'Resumen' },
        {
          k: 'table',
          caption: 'Avisos publicados. Severidad: crítica cuando la capacidad o la demanda podían quedar muy del lado inseguro sin señal visible.',
          head: ['Aviso', 'Qué', 'Severidad', 'Estado'],
          rows: [
            ['SA-2026-013', 'Láminas curvas: momentos flectores informados como cero; tensiones con la deformación térmica libre', 'Alta', 'Corregido en main'],
            ['SA-2026-011', 'Apoyo 3D cambiado de resorte a empotrado o articulado, calculado como resorte', 'Alta', 'Corregido el 20/10/2026'],
            ['SA-2026-008', 'Espectro de respuesta: corte basal SRSS con CQC; participación sin la masa acoplada a los apoyos', 'Alta', 'Corregido el 21/10/2026'],
            ['SA-2026-007', 'Pandeo 3D sin modos torsionales ni flexotorsionales', 'Alta', 'Corregido el 21/10/2026'],
            ['SA-2026-006', 'Modelo plano resuelto en 3D: momentos nodales y giros impuestos invertidos', 'Alta', 'Corregido el 21/10/2026'],
            ['SA-2026-005', 'Barras inclinadas 2D: esfuerzo axil sin la componente de la carga a lo largo de la barra', 'Alta', 'Corregido el 21/10/2026'],
            ['SA-2026-012', 'Análisis no lineal de material (sólo motor y WASM): esfuerzos de barra elásticos tras la fluencia', 'Media', 'Corregido en main'],
            ['SA-2026-010', 'Certificado espectral aprobado con resultados no finitos', 'Media', 'Corregido el 19/10/2026'],
            ['SA-2026-009', 'Sólido-lámina SHB8 un 22 % demasiado rígido en flexión de placas delgadas (flechas un 18 % bajas)', 'Media (mayor en estructuras hiperestáticas)', 'Corregido el 19/10/2026'],
            ['SA-2026-004', 'Análisis que ignoraban parte del modelo (dinámica, pandeo, colapso plástico)', 'Crítica', 'Corregido en 0.2.0'],
            ['SA-2026-003', 'Verificaciones de acero, perfiles conformados y uniones con capacidad sobrestimada', 'Crítica', 'Corregido en 0.2.0'],
            ['SA-2026-002', 'Verificaciones de hormigón (CIRSOC 201, ACI 318) con capacidad sobrestimada', 'Crítica', 'Corregido en 0.2.0'],
            ['SA-2026-001', 'Combinaciones y cargas generadas (CIRSOC 101, 102, 103) del lado inseguro', 'Alta', 'Corregido en 0.2.0'],
          ],
        },
        {
          k: 'table',
          caption: 'Casos documentados en cada aviso: el valor correcto y el que informaba Stabileo.',
          head: ['Aviso', 'Caso', 'Correcto', 'Informado', 'Unidad'],
          rows: [
            ['SA-2026-013', 'Ménsula de cuatro láminas curvas de 4 × 1 × 0,2 m, 10 kN en la punta: mx en el elemento del empotramiento', '35,0', '0', 'kN·m/m'],
            ['SA-2026-011', 'Viga de 0, 4 y 8 m, apoyo dibujado como empotrado en 8 m: descenso en ese apoyo', '0', '-14,4', 'mm'],
            ['SA-2026-008', 'Dos modos a ω y 1,1ω: corte basal relativo al valor CQC', '1,00', '0,81', '—'],
            ['SA-2026-007', 'Columna cruciforme de 200×10 mm y 1,5 m: carga crítica', '2993', '5860', 'kN'],
            ['SA-2026-006', 'Pórtico con momentos y fuerzas nodales: desplazamiento lateral', '0,92', '2,52', 'mm'],
            ['SA-2026-005', 'Cabio de techo a dos aguas: esfuerzo axil en la base', '-56,67', '-41,67', 'kN'],
            ['SA-2026-012', 'Ménsula de 4 m fluida, carga en la punta 1,2·Mp/L: momento en el empotramiento', '1012,5', '101250', 'kN·m'],
            ['SA-2026-009', 'Placa delgada en flexión, ν = 0,3: rigidez relativa a la correcta', '1,000', '1,225', '—'],
          ],
        },

        { k: 'h', t: 'SA-2026-011: apoyo 3D cambiado de resorte a empotrado' },
        { k: 'p', t: 'Al cambiar el tipo de un apoyo que tenía resortes, el apoyo conservaba las constantes de resorte. En modo 3D se enviaban al solver junto con la restricción, y el solver resolvía ese grado de libertad como elástico: el apoyo dibujado como empotrado se calculaba como un resorte. Afecta los modelos resueltos en modo 3D, incluidos los planos resueltos en 3D; el modo 2D no está afectado.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: algún apoyo fue primero un resorte (o tuvo constantes de resorte) y después se cambió a empotrado, articulado o rodillo, o se le tildó un grado de libertad como restringido. En los resultados, ese apoyo muestra un desplazamiento o un giro distinto de cero.',
            'Qué hacer: volvé a resolver en stabileo.com los modelos 3D con apoyos editados de resorte a rígido y compará los resultados cerca de esos apoyos con los que ya usaste.',
            'Estado: corregido en stabileo.com el 20 de octubre de 2026. Todo grado de libertad restringido se resuelve como restringido, también en los proyectos guardados antes.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-011-springs-on-fixed-supports.md`, label: 'SA-2026-011 completo', t: 'El aviso con la reproducción y el modelo de prueba:' },

        { k: 'h', t: 'SA-2026-013: momentos y tensiones térmicas de las láminas curvas' },
        { k: 'p', t: 'Las láminas curvas (los cuadriláteros marcados como curvos) informaban mx = my = mxy = 0 cualquiera fuera la flexión, y bajo una carga de temperatura sus tensiones conservaban la deformación térmica libre: una lámina libre de dilatarse informaba E·α·ΔT/(1−ν) y una restringida, nada. Además, la aplicación enviaba la temperatura de un cuadrilátero curvo como la de uno plano, y el motor la descartaba: la lámina se calculaba sin calentar. Lo leían las curvas de nivel de láminas (también en las figuras exportadas), la tabla de resultados y el informe PDF de Pro, y el diseño de losas si un paño era un cuadrilátero curvo; las tablas del libro de resultados y el CSV ya dejaban esos momentos en blanco.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: el modelo tiene cuadriláteros curvos y usaste sus momentos (curvas de nivel, tabla o informe de Pro, diseño de losas), o tiene una carga de temperatura sobre un cuadrilátero curvo.',
            'Qué hacer: volvé a resolver esos modelos y repetí el diseño o la verificación que leyó sus momentos o sus tensiones bajo temperatura. Mientras tanto, modelá la superficie con cuadriláteros planos (MITC4).',
            'Estado: corregido en main; se publica en stabileo.com con su despliegue.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-013-curved-shell-moments-and-thermal.md`, label: 'SA-2026-013 completo', t: 'El aviso, con la ménsula de prueba y los casos térmicos:' },

        { k: 'h', t: 'SA-2026-012: esfuerzos de barra en el análisis no lineal de material' },
        { k: 'p', t: 'Después de que una barra fluía, el solver equilibraba las cargas con la rigidez reducida pero informaba los esfuerzos calculados con la rigidez elástica. Desplazamientos y reacciones eran correctos; los esfuerzos de barra y la utilización no: en una ménsula, el momento informado era 100 veces el real. Este solver sólo se usa desde el motor o su API WASM: ninguna pantalla de la aplicación lo llama, así que los resultados obtenidos en stabileo.com no están afectados.' },
        { k: 'ext', href: `${REPO}/SA-2026-012-material-nonlinear-member-forces.md`, label: 'SA-2026-012 completo', t: 'El aviso, con la ménsula de prueba:' },

        { k: 'h', t: 'SA-2026-008: corte basal y participación modal en el análisis espectral' },
        { k: 'p', t: 'Con la regla CQC, el corte basal informado se combinaba igual con SRSS. Los cortes basales de los modos en una dirección tienen el mismo signo, así que CQC da más que SRSS: con dos modos cercanos el valor informado quedaba un 19 % bajo. Además, los factores de participación omitían la masa que la matriz de masa consistente acopla a los apoyos, lo que subestimaba la respuesta en modelos con pocas barras por columna.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: hiciste un análisis espectral con CQC y usaste el corte basal informado (por ejemplo, para escalarlo al corte estático), o usaste masa consistente con columnas de uno o dos elementos.',
            'Qué hacer: volvé a resolver en stabileo.com los modelos afectados y compará con los resultados que ya usaste. La alternativa que este aviso daba antes de la corrección ya no corresponde.',
            'Estado: corregido en stabileo.com el 21 de octubre de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-008-spectral-combination.md`, label: 'SA-2026-008 completo', t: 'El aviso, con los dos defectos por separado:' },

        { k: 'h', t: 'SA-2026-007: pandeo torsional y flexotorsional en 3D' },
        { k: 'p', t: 'La rigidez geométrica de las barras 3D tenía sólo los términos de flexión: no incluía el término de Wagner ni el acoplamiento con el centro de corte. El pandeo torsional y el flexotorsional nunca aparecían, y se informaba la carga de Euler. En una columna cruciforme la carga informada era un 96 % mayor que la que gobierna.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: usaste el factor de pandeo 3D o el análisis P-Delta 3D en estructuras con secciones abiertas de pared delgada y poca rigidez torsional: cruciformes, ángulos simples o dobles, tes, canales. Las secciones doble te con simetría doble, en las que gobierna la flexión, no están afectadas. Las verificaciones de diseño que calculan el pandeo torsional por su cuenta (CIRSOC 301 E.4, desde 0.2.0) tampoco.',
            'Qué hacer: volvé a resolver en stabileo.com los modelos afectados y compará con los resultados que ya usaste. La alternativa que este aviso daba antes de la corrección ya no corresponde.',
            'Estado: corregido en stabileo.com el 21 de octubre de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-007-torsional-buckling.md`, label: 'SA-2026-007 completo', t: 'El aviso, con el cálculo de la columna cruciforme:' },

        { k: 'h', t: 'SA-2026-006: modelo plano resuelto en 3D con momentos nodales' },
        { k: 'p', t: 'Un modelo plano resuelto con el solver 3D se ubica en el plano X–Z, donde el giro del plano es un giro alrededor de −Y. Los momentos nodales y los giros impuestos debían cambiar de signo y no lo hacían, así que actuaban como su imagen especular. Combinados con fuerzas, pueden sumarse en lugar de restarse.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: el modelo es plano (todos los nodos en un plano), se resolvió en modo 3D y tiene momentos nodales o giros impuestos en apoyos. Los resortes rotacionales no están afectados.',
            'Qué hacer: volvé a resolver en stabileo.com los modelos afectados y compará con los resultados que ya usaste. La alternativa que este aviso daba antes de la corrección ya no corresponde.',
            'Estado: corregido en stabileo.com el 21 de octubre de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-006-mirrored-moments-2d-in-3d.md`, label: 'SA-2026-006 completo', t: 'El aviso, con los tres casos de prueba:' },

        { k: 'h', t: 'SA-2026-005: esfuerzo axil en barras inclinadas 2D' },
        { k: 'p', t: 'El armado de cargas 2D pasaba a los nodos extremos la componente de la carga a lo largo de una barra inclinada. Reacciones y desplazamientos eran correctos, pero la barra no veía esa carga en el tramo, así que su esfuerzo axil quedaba constante, igual al promedio de los valores reales en sus extremos: en la base de un cabio, un 26 % bajo. El diagrama, el inspector, la memoria, la exportación a Excel y el dimensionado usaban ese valor. Además, un par aplicado sobre una barra se perdía cuando un modelo plano se resolvía en 3D.' },
        {
          k: 'ul',
          items: [
            'Cómo saber si te afecta: un modelo 2D resuelto desde el 25/09/2026 con barras inclinadas (cabios, escaleras, diagonales, arcos por segmentos) que llevan cargas distribuidas, puntuales o peso propio con componente a lo largo de la barra; o un modelo plano con un par sobre una barra resuelto en 3D. Las barras dimensionadas a compresión son las más expuestas.',
            'Qué hacer: volvé a resolver en stabileo.com los modelos afectados y compará con los resultados que ya usaste. La alternativa que este aviso daba antes de la corrección ya no corresponde.',
            'Estado: corregido en stabileo.com el 21 de octubre de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-005-inclined-member-loads.md`, label: 'SA-2026-005 completo', t: 'El aviso, con el cabio y la barra 3-4-5:' },

        { k: 'h', t: 'Corregidos el 19 de octubre de 2026' },
        {
          k: 'ul',
          items: [
            'SA-2026-009: el elemento sólido-lámina SHB8 era un 22 % demasiado rígido en flexión de placas delgadas (bloqueo de Poisson en el espesor), así que subestimaba las flechas en alrededor de un 18 %. Revisá las conclusiones de flecha y vibración de modelos con placas de sólido-lámina resueltos antes de esa fecha; MITC4, MITC9, DKT y la lámina curva no estaban afectados.',
            'SA-2026-010: el certificado del análisis espectral podía aprobar resultados no finitos (NaN). Los números eran visiblemente inválidos; lo equivocado era la afirmación de verificación. No hace falta revisar nada salvo un resultado entregado que mostrara valores no finitos.',
          ],
        },

        { k: 'h', t: 'Corregidos en la versión 0.2.0' },
        { k: 'p', t: 'La versión 0.2.0, del 18 de octubre de 2026, corrigió los cuatro avisos retroactivos. Todos afectan proyectos resueltos antes del 17 de octubre de 2026 y su recomendación es la misma: volver a generar cargas y combinaciones, resolver y revisar las verificaciones, y revisar cualquier entregable emitido con una versión anterior.' },
        {
          k: 'ul',
          items: [
            'SA-2026-001: combinaciones y cargas generadas CIRSOC 101, 102 y 103 del lado inseguro (en un ejemplo, el axil de una columna era un 29 % bajo).',
            'SA-2026-002: verificaciones de hormigón CIRSOC 201 y ACI 318 con capacidad sobrestimada (φMn hasta un 18 % alto).',
            'SA-2026-003: verificaciones de acero, perfiles conformados y uniones con capacidad sobrestimada (corte del alma hasta un 60 % alto).',
            'SA-2026-004: análisis que ignoraban parte del modelo (el sismo con registro de aceleraciones perdía hasta la mitad de la carga; el colapso plástico 3D, hasta 2,3 veces del lado inseguro).',
          ],
        },
        { k: 'ext', href: `${REPO}/README.md`, label: 'Todos los avisos', t: 'La lista completa, con cada aviso, su corrección y su severidad:' },
        { k: 'link', slug: 'how-stabileo-checks-its-results', t: 'Cómo encontramos estos errores, y qué no puede garantizar ninguna verificación:' },
      ],
    },
    en: {
      title: 'Safety advisories: which results changed and what to do',
      excerpt:
        'The Stabileo defects that could give wrong or unconservative results: who they affect, how to tell whether a model is affected, what to do in the meantime and when they were fixed.',
      blocks: [
        { k: 'p', t: 'Stabileo’s results are used to design real structures. When we find a defect that could have produced a wrong result, and especially an unconservative one, we publish it as a safety advisory: what was wrong, which models it affects, how to recognise them, what to do until the fix is live, and what to re-check in results that were already used.' },
        { k: 'p', t: 'Almost all of these defects were found by our own verification programme: comparison against seven other finite-element programs, published worked examples, physical-invariant tests and a review of every element’s derivation. Publishing them is part of that work: a defect that is not reported stays in the calculations that were already delivered.' },
        { k: 'note', t: 'Status as of 21 October 2026. An advisory counts as fixed when the fix is live on stabileo.com, not when the change exists. While an advisory is open, use its workaround and check the affected result by hand before signing.' },

        { k: 'h', t: 'Summary' },
        {
          k: 'table',
          caption: 'Published advisories. Severity: critical when capacity or demand could be far on the unsafe side with no visible sign.',
          head: ['Advisory', 'What', 'Severity', 'Status'],
          rows: [
            ['SA-2026-013', 'Curved shells: bending moments reported as zero; stresses kept the free thermal strain', 'High', 'Fixed on main'],
            ['SA-2026-011', '3D support switched from spring to fixed or pinned, solved as a spring', 'High', 'Fixed 2026-10-20'],
            ['SA-2026-008', 'Response spectrum: base shear SRSS under CQC; participation without support-coupled mass', 'High', 'Fixed 2026-10-21'],
            ['SA-2026-007', '3D buckling without torsional or flexural-torsional modes', 'High', 'Fixed 2026-10-21'],
            ['SA-2026-006', 'Plane model solved in 3D: nodal moments and imposed rotations reversed', 'High', 'Fixed 2026-10-21'],
            ['SA-2026-005', '2D inclined members: axial force without the along-member load component', 'High', 'Fixed 2026-10-21'],
            ['SA-2026-012', 'Material-nonlinear analysis (engine and WASM only): elastic member forces after yield', 'Medium', 'Fixed on main'],
            ['SA-2026-010', 'Spectral certificate passed non-finite results', 'Medium', 'Fixed 2026-10-19'],
            ['SA-2026-009', 'SHB8 solid shell 22 % too stiff in thin-plate bending (deflections 18 % low)', 'Medium (higher in indeterminate structures)', 'Fixed 2026-10-19'],
            ['SA-2026-004', 'Analyses that ignored part of the model (dynamics, buckling, plastic collapse)', 'Critical', 'Fixed in 0.2.0'],
            ['SA-2026-003', 'Steel, cold-formed and connection checks overstated capacity', 'Critical', 'Fixed in 0.2.0'],
            ['SA-2026-002', 'Concrete checks (CIRSOC 201, ACI 318) overstated capacity', 'Critical', 'Fixed in 0.2.0'],
            ['SA-2026-001', 'Generated combinations and loads (CIRSOC 101, 102, 103) unconservative', 'High', 'Fixed in 0.2.0'],
          ],
        },
        {
          k: 'table',
          caption: 'Cases documented in each advisory: the correct value and the one Stabileo reported.',
          head: ['Advisory', 'Case', 'Correct', 'Reported', 'Unit'],
          rows: [
            ['SA-2026-013', 'Cantilever of four curved shells, 4 × 1 × 0.2 m, 10 kN at the tip: mx at the root element', '35.0', '0', 'kN·m/m'],
            ['SA-2026-011', 'Beam at 0, 4 and 8 m, support drawn as fixed at 8 m: deflection at that support', '0', '-14.4', 'mm'],
            ['SA-2026-008', 'Two modes at ω and 1.1ω: base shear relative to the CQC value', '1.00', '0.81', '—'],
            ['SA-2026-007', '200×10 mm cruciform column, 1.5 m: critical load', '2993', '5860', 'kN'],
            ['SA-2026-006', 'Portal with nodal moments and forces: sway', '0.92', '2.52', 'mm'],
            ['SA-2026-005', 'Gable rafter: axial force at the base', '-56.67', '-41.67', 'kN'],
            ['SA-2026-012', 'Yielded 4 m cantilever, tip load 1.2·Mp/L: moment at the fixed end', '1012.5', '101250', 'kN·m'],
            ['SA-2026-009', 'Thin plate in bending, ν = 0.3: stiffness relative to the correct value', '1.000', '1.225', '—'],
          ],
        },

        { k: 'h', t: 'SA-2026-011: 3D support switched from spring to fixed' },
        { k: 'p', t: 'Changing the type of a support that had springs kept its spring constants. In 3D mode they were sent to the solver along with the restraint, and the solver treated that degree of freedom as elastic: the support drawn as fixed was solved as a spring. It affects models solved in 3D mode, including plane models solved in 3D; 2D mode is not affected.' },
        {
          k: 'ul',
          items: [
            'How to tell if you are affected: a support was first a spring (or had spring constants) and was then changed to fixed, pinned or a roller, or had a degree of freedom ticked as restrained. In the results, that support shows a non-zero displacement or rotation.',
            'What to do: re-solve on stabileo.com the 3D models whose supports were edited from springs to rigid, and compare the results near those supports with the ones you already used.',
            'Status: fixed on stabileo.com on 20 October 2026. Every restrained degree of freedom is solved as restrained, including in projects saved before.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-011-springs-on-fixed-supports.md`, label: 'Full SA-2026-011', t: 'The advisory with the reproduction and the test model:' },

        { k: 'h', t: 'SA-2026-013: curved-shell moments and thermal stresses' },
        { k: 'p', t: 'Curved shells (quads marked as curved) reported mx = my = mxy = 0 whatever the bending, and under a temperature load their stresses kept the free thermal strain: a shell free to expand reported E·α·ΔT/(1−ν), and a restrained one nothing. The app also sent a curved quad’s temperature as a flat quad’s, which the engine dropped: the shell was analysed unheated. These results were read by the shell contour plots (also in exported figures), the Pro results table and PDF report, and the slab design when a panel was a curved quad; the results workbook and CSV already left those moments blank.' },
        {
          k: 'ul',
          items: [
            'How to tell if it affects you: the model has curved quads and you used their moments (contours, the Pro table or report, slab design), or it has a temperature load on a curved quad.',
            'What to do: re-run those models and repeat any design or check that read their moments or their stresses under temperature. Until then, model the surface with flat quads (MITC4).',
            'Status: fixed on main; it reaches stabileo.com with its deployment.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-013-curved-shell-moments-and-thermal.md`, label: 'Full SA-2026-013', t: 'The advisory, with the test cantilever and the thermal cases:' },

        { k: 'h', t: 'SA-2026-012: member forces in material-nonlinear analysis' },
        { k: 'p', t: 'After a member yielded, the solver balanced the loads with the reduced stiffness but reported forces computed with the elastic stiffness. Displacements and reactions were right; member forces and utilization were not: in a cantilever the reported moment was 100 times the real one. This solver is only reachable from the engine or its WASM API: no screen in the app calls it, so results obtained on stabileo.com are not affected.' },
        { k: 'ext', href: `${REPO}/SA-2026-012-material-nonlinear-member-forces.md`, label: 'Full SA-2026-012', t: 'The advisory, with the test cantilever:' },

        { k: 'h', t: 'SA-2026-008: base shear and modal participation in spectral analysis' },
        { k: 'p', t: 'With the CQC rule, the reported base shear was still combined with SRSS. The modal base shears in one direction share a sign, so CQC gives more than SRSS: with two close modes the reported value was 19 % low. The participation factors also omitted the mass that the consistent mass matrix couples to the supports, which underestimated the response in models with few elements per column.' },
        {
          k: 'ul',
          items: [
            'How to tell if you are affected: you ran a spectral analysis with CQC and used the reported base shear (for example, to scale it to the static base shear), or you used consistent mass with columns of one or two elements.',
            'What to do: re-solve the affected models on stabileo.com and compare with the results you already used. The workaround this advisory gave before the fix no longer applies.',
            'Status: fixed on stabileo.com on 21 October 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-008-spectral-combination.md`, label: 'Full SA-2026-008', t: 'The advisory, with the two defects separately:' },

        { k: 'h', t: 'SA-2026-007: torsional and flexural-torsional buckling in 3D' },
        { k: 'p', t: 'The geometric stiffness of 3D members had only the bending terms: no Wagner term and no shear-centre coupling. Torsional and flexural-torsional buckling never appeared, and the Euler load was reported instead. For a cruciform column the reported load was 96 % above the governing one.' },
        {
          k: 'ul',
          items: [
            'How to tell if you are affected: you used the 3D buckling factor or 3D P-Delta for structures with open thin-walled sections of low torsional stiffness: cruciforms, single or double angles, tees, channels. Doubly symmetric I-sections, where flexural buckling governs, are not affected. Design checks that compute torsional buckling themselves (CIRSOC 301 E.4, since 0.2.0) are not affected either.',
            'What to do: re-solve the affected models on stabileo.com and compare with the results you already used. The workaround this advisory gave before the fix no longer applies.',
            'Status: fixed on stabileo.com on 21 October 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-007-torsional-buckling.md`, label: 'Full SA-2026-007', t: 'The advisory, with the cruciform column worked out:' },

        { k: 'h', t: 'SA-2026-006: plane model solved in 3D with nodal moments' },
        { k: 'p', t: 'A plane model solved by the 3D solver stands in the X–Z plane, where the plane rotation is a rotation about −Y. Nodal moments and imposed rotations had to change sign and did not, so they acted as their mirror image. Combined with forces, they can add instead of cancel.' },
        {
          k: 'ul',
          items: [
            'How to tell if you are affected: the model is flat (all nodes in one plane), was solved in 3D mode and has nodal moments or imposed support rotations. Rotational springs are not affected.',
            'What to do: re-solve the affected models on stabileo.com and compare with the results you already used. The workaround this advisory gave before the fix no longer applies.',
            'Status: fixed on stabileo.com on 21 October 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-006-mirrored-moments-2d-in-3d.md`, label: 'Full SA-2026-006', t: 'The advisory, with the three test cases:' },

        { k: 'h', t: 'SA-2026-005: axial force in 2D inclined members' },
        { k: 'p', t: 'The 2D load builder moved the along-member component of a load on an inclined member to its end nodes. Reactions and displacements were right, but the member did not see that load in its span, so its axial force stayed constant at the mean of its true end values: 26 % low at the base of a rafter. The diagram, the inspector, the report, the Excel export and the design all used that value. A couple applied on a member was also dropped when a plane model was solved in 3D.' },
        {
          k: 'ul',
          items: [
            'How to tell if you are affected: a 2D model solved since 25/09/2026 with inclined members (rafters, stairs, braces, arches drawn as segments) carrying distributed, point or self-weight loads with a component along the member; or a plane model with a couple on a member solved in 3D. Members designed for compression are the most exposed.',
            'What to do: re-solve the affected models on stabileo.com and compare with the results you already used. The workaround this advisory gave before the fix no longer applies.',
            'Status: fixed on stabileo.com on 21 October 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-005-inclined-member-loads.md`, label: 'Full SA-2026-005', t: 'The advisory, with the rafter and the 3-4-5 member:' },

        { k: 'h', t: 'Fixed on 19 October 2026' },
        {
          k: 'ul',
          items: [
            'SA-2026-009: the SHB8 solid-shell element was 22 % too stiff in thin-plate bending (Poisson thickness locking), so it underestimated deflections by about 18 %. Re-check deflection and vibration conclusions for models with solid-shell plates solved before that date; MITC4, MITC9, DKT and the curved shell were not affected.',
            'SA-2026-010: the spectral analysis certificate could pass non-finite results (NaN). The numbers were visibly invalid; what was wrong was the verification claim. Nothing needs re-checking unless a delivered result showed non-finite values.',
          ],
        },

        { k: 'h', t: 'Fixed in version 0.2.0' },
        { k: 'p', t: 'Version 0.2.0, of 18 October 2026, fixed the four retroactive advisories. They all affect projects solved before 17 October 2026, and the advice is the same for each: regenerate loads and combinations, solve and review the design checks, and re-check any deliverable issued with an earlier version.' },
        {
          k: 'ul',
          items: [
            'SA-2026-001: generated CIRSOC 101, 102 and 103 combinations and loads on the unsafe side (in one example a column’s axial force was 29 % low).',
            'SA-2026-002: CIRSOC 201 and ACI 318 concrete checks overstated capacity (φMn up to 18 % high).',
            'SA-2026-003: steel, cold-formed and connection checks overstated capacity (web shear up to 60 % high).',
            'SA-2026-004: analyses that ignored part of the model (ground-motion time history lost up to half the earthquake load; 3D plastic collapse up to 2.3 times unsafe).',
          ],
        },
        { k: 'ext', href: `${REPO}/README.md`, label: 'All advisories', t: 'The full list, with each advisory, its fix and its severity:' },
        { k: 'link', slug: 'how-stabileo-checks-its-results', t: 'How we found these defects, and what no verification can guarantee:' },
      ],
    },
    pt: {
      title: 'Avisos de segurança: quais resultados mudaram e o que fazer',
      excerpt:
        'Os defeitos do Stabileo que podiam dar resultados errados ou contra a segurança: a quem afetam, como saber se um modelo foi afetado, o que fazer enquanto isso e quando foram corrigidos.',
      blocks: [
        { k: 'p', t: 'Os resultados do Stabileo são usados para dimensionar estruturas reais. Quando encontramos um defeito que pode ter produzido um resultado errado, e em especial um contra a segurança, publicamos um aviso de segurança: o que estava errado, quais modelos afeta, como reconhecê-los, o que fazer até a correção estar publicada e o que revisar nos resultados que já foram usados.' },
        { k: 'p', t: 'Quase todos esses defeitos foram encontrados pelo nosso próprio programa de verificação: comparação com outros sete programas de elementos finitos, exemplos resolvidos publicados, testes de invariantes físicos e revisão da derivação de cada elemento. Publicá-los faz parte desse trabalho: um defeito que não é informado continua nos cálculos já entregues.' },
        { k: 'note', t: 'Situação em 21 de outubro de 2026. Um aviso conta como corrigido quando a correção está publicada em stabileo.com, não quando a mudança existe. Enquanto um aviso estiver aberto, use a alternativa indicada e confira à mão o resultado afetado antes de assinar.' },

        { k: 'h', t: 'Resumo' },
        {
          k: 'table',
          caption: 'Avisos publicados. Severidade: crítica quando a capacidade ou a solicitação podiam ficar muito contra a segurança sem sinal visível.',
          head: ['Aviso', 'O quê', 'Severidade', 'Situação'],
          rows: [
            ['SA-2026-013', 'Cascas curvas: momentos fletores informados como zero; tensões com a deformação térmica livre', 'Alta', 'Corrigido no main'],
            ['SA-2026-011', 'Apoio 3D trocado de mola para engaste ou rótula, calculado como mola', 'Alta', 'Corrigido em 20/10/2026'],
            ['SA-2026-008', 'Espectro de resposta: cortante basal SRSS com CQC; participação sem a massa acoplada aos apoios', 'Alta', 'Corrigido em 21/10/2026'],
            ['SA-2026-007', 'Flambagem 3D sem modos torcionais nem flexo-torcionais', 'Alta', 'Corrigido em 21/10/2026'],
            ['SA-2026-006', 'Modelo plano resolvido em 3D: momentos nodais e rotações impostas invertidos', 'Alta', 'Corrigido em 21/10/2026'],
            ['SA-2026-005', 'Barras inclinadas 2D: esforço normal sem a componente da carga ao longo da barra', 'Alta', 'Corrigido em 21/10/2026'],
            ['SA-2026-012', 'Análise não linear de material (só motor e WASM): esforços de barra elásticos após o escoamento', 'Média', 'Corrigido no main'],
            ['SA-2026-010', 'Certificado espectral aprovado com resultados não finitos', 'Média', 'Corrigido em 19/10/2026'],
            ['SA-2026-009', 'Sólido-casca SHB8 22 % rígido demais na flexão de placas finas (flechas 18 % baixas)', 'Média (maior em estruturas hiperestáticas)', 'Corrigido em 19/10/2026'],
            ['SA-2026-004', 'Análises que ignoravam parte do modelo (dinâmica, flambagem, colapso plástico)', 'Crítica', 'Corrigido na 0.2.0'],
            ['SA-2026-003', 'Verificações de aço, perfis formados a frio e ligações com capacidade superestimada', 'Crítica', 'Corrigido na 0.2.0'],
            ['SA-2026-002', 'Verificações de concreto (CIRSOC 201, ACI 318) com capacidade superestimada', 'Crítica', 'Corrigido na 0.2.0'],
            ['SA-2026-001', 'Combinações e cargas geradas (CIRSOC 101, 102, 103) contra a segurança', 'Alta', 'Corrigido na 0.2.0'],
          ],
        },
        {
          k: 'table',
          caption: 'Casos documentados em cada aviso: o valor correto e o que o Stabileo informava.',
          head: ['Aviso', 'Caso', 'Correto', 'Informado', 'Unidade'],
          rows: [
            ['SA-2026-013', 'Balanço de quatro cascas curvas de 4 × 1 × 0,2 m, 10 kN na ponta: mx no elemento do engaste', '35,0', '0', 'kN·m/m'],
            ['SA-2026-011', 'Viga em 0, 4 e 8 m, apoio desenhado como engaste em 8 m: deslocamento nesse apoio', '0', '-14,4', 'mm'],
            ['SA-2026-008', 'Dois modos em ω e 1,1ω: cortante basal relativo ao valor CQC', '1,00', '0,81', '—'],
            ['SA-2026-007', 'Pilar cruciforme de 200×10 mm e 1,5 m: carga crítica', '2993', '5860', 'kN'],
            ['SA-2026-006', 'Pórtico com momentos e forças nodais: deslocamento lateral', '0,92', '2,52', 'mm'],
            ['SA-2026-005', 'Caibro de telhado de duas águas: esforço normal na base', '-56,67', '-41,67', 'kN'],
            ['SA-2026-012', 'Balanço de 4 m escoado, carga na ponta 1,2·Mp/L: momento no engaste', '1012,5', '101250', 'kN·m'],
            ['SA-2026-009', 'Placa fina em flexão, ν = 0,3: rigidez relativa à correta', '1,000', '1,225', '—'],
          ],
        },

        { k: 'h', t: 'SA-2026-011: apoio 3D trocado de mola para engaste' },
        { k: 'p', t: 'Ao trocar o tipo de um apoio que tinha molas, o apoio mantinha as constantes de mola. No modo 3D elas iam para o solver junto com a restrição, e o solver tratava esse grau de liberdade como elástico: o apoio desenhado como engaste era calculado como mola. Afeta modelos resolvidos no modo 3D, inclusive modelos planos resolvidos em 3D; o modo 2D não é afetado.' },
        {
          k: 'ul',
          items: [
            'Como saber se você foi afetado: algum apoio foi primeiro uma mola (ou teve constantes de mola) e depois foi trocado para engaste, rótula ou rolete, ou teve um grau de liberdade marcado como restringido. Nos resultados, esse apoio mostra deslocamento ou rotação diferente de zero.',
            'O que fazer: resolva de novo em stabileo.com os modelos 3D com apoios editados de mola para rígido e compare os resultados perto desses apoios com os que você já usou.',
            'Situação: corrigido em stabileo.com em 20 de outubro de 2026. Todo grau de liberdade restringido é resolvido como restringido, inclusive em projetos salvos antes.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-011-springs-on-fixed-supports.md`, label: 'SA-2026-011 completo', t: 'O aviso com a reprodução e o modelo de teste:' },

        { k: 'h', t: 'SA-2026-013: momentos e tensões térmicas das cascas curvas' },
        { k: 'p', t: 'As cascas curvas (os quadriláteros marcados como curvos) informavam mx = my = mxy = 0 qualquer que fosse a flexão, e sob uma carga de temperatura suas tensões mantinham a deformação térmica livre: uma casca livre para dilatar informava E·α·ΔT/(1−ν), e uma restringida, nada. Além disso, o aplicativo enviava a temperatura de um quadrilátero curvo como a de um plano, e o motor a descartava: a casca era calculada sem aquecimento. Esses resultados eram lidos pelas curvas de nível de cascas (também nas figuras exportadas), pela tabela de resultados e pelo relatório PDF do Pro, e pelo dimensionamento de lajes quando um painel era um quadrilátero curvo; as tabelas da pasta de resultados e o CSV já deixavam esses momentos em branco.' },
        {
          k: 'ul',
          items: [
            'Como saber se afeta você: o modelo tem quadriláteros curvos e você usou seus momentos (curvas de nível, tabela ou relatório do Pro, dimensionamento de lajes), ou tem uma carga de temperatura num quadrilátero curvo.',
            'O que fazer: resolva de novo esses modelos e repita o dimensionamento ou a verificação que leu seus momentos ou suas tensões sob temperatura. Enquanto isso, modele a superfície com quadriláteros planos (MITC4).',
            'Situação: corrigido no main; chega ao stabileo.com com sua implantação.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-013-curved-shell-moments-and-thermal.md`, label: 'SA-2026-013 completo', t: 'O aviso, com o balanço de teste e os casos térmicos:' },

        { k: 'h', t: 'SA-2026-012: esforços de barra na análise não linear de material' },
        { k: 'p', t: 'Depois que uma barra escoava, o solver equilibrava as cargas com a rigidez reduzida, mas informava os esforços calculados com a rigidez elástica. Deslocamentos e reações estavam corretos; esforços de barra e utilização não: num balanço, o momento informado era 100 vezes o real. Este solver só é usado a partir do motor ou da sua API WASM: nenhuma tela do aplicativo o chama, então os resultados obtidos em stabileo.com não são afetados.' },
        { k: 'ext', href: `${REPO}/SA-2026-012-material-nonlinear-member-forces.md`, label: 'SA-2026-012 completo', t: 'O aviso, com o balanço de teste:' },

        { k: 'h', t: 'SA-2026-008: cortante basal e participação modal na análise espectral' },
        { k: 'p', t: 'Com a regra CQC, o cortante basal informado continuava combinado com SRSS. Os cortantes basais modais numa direção têm o mesmo sinal, então CQC dá mais que SRSS: com dois modos próximos o valor informado ficava 19 % baixo. Além disso, os fatores de participação omitiam a massa que a matriz de massa consistente acopla aos apoios, o que subestimava a resposta em modelos com poucos elementos por pilar.' },
        {
          k: 'ul',
          items: [
            'Como saber se você foi afetado: fez uma análise espectral com CQC e usou o cortante basal informado (por exemplo, para escalá-lo ao cortante estático), ou usou massa consistente com pilares de um ou dois elementos.',
            'O que fazer: resolva de novo em stabileo.com os modelos afetados e compare com os resultados que você já usou. A alternativa que este aviso dava antes da correção não se aplica mais.',
            'Situação: corrigido em stabileo.com em 21 de outubro de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-008-spectral-combination.md`, label: 'SA-2026-008 completo', t: 'O aviso, com os dois defeitos separados:' },

        { k: 'h', t: 'SA-2026-007: flambagem torcional e flexo-torcional em 3D' },
        { k: 'p', t: 'A rigidez geométrica das barras 3D tinha só os termos de flexão: sem o termo de Wagner e sem o acoplamento com o centro de cisalhamento. A flambagem torcional e a flexo-torcional nunca apareciam, e era informada a carga de Euler. Num pilar cruciforme, a carga informada era 96 % maior que a que governa.' },
        {
          k: 'ul',
          items: [
            'Como saber se você foi afetado: usou o fator de flambagem 3D ou o P-Delta 3D em estruturas com seções abertas de parede fina e pouca rigidez à torção: cruciformes, cantoneiras simples ou duplas, tês, perfis U. Perfis I com dupla simetria, em que governa a flexão, não são afetados. As verificações de projeto que calculam a flambagem torcional por conta própria (CIRSOC 301 E.4, desde a 0.2.0) também não.',
            'O que fazer: resolva de novo em stabileo.com os modelos afetados e compare com os resultados que você já usou. A alternativa que este aviso dava antes da correção não se aplica mais.',
            'Situação: corrigido em stabileo.com em 21 de outubro de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-007-torsional-buckling.md`, label: 'SA-2026-007 completo', t: 'O aviso, com o cálculo do pilar cruciforme:' },

        { k: 'h', t: 'SA-2026-006: modelo plano resolvido em 3D com momentos nodais' },
        { k: 'p', t: 'Um modelo plano resolvido pelo solver 3D fica no plano X–Z, onde a rotação do plano é uma rotação em torno de −Y. Os momentos nodais e as rotações impostas deviam trocar de sinal e não trocavam, então agiam como sua imagem espelhada. Combinados com forças, podem somar em vez de subtrair.' },
        {
          k: 'ul',
          items: [
            'Como saber se você foi afetado: o modelo é plano (todos os nós num plano), foi resolvido no modo 3D e tem momentos nodais ou rotações impostas nos apoios. As molas rotacionais não são afetadas.',
            'O que fazer: resolva de novo em stabileo.com os modelos afetados e compare com os resultados que você já usou. A alternativa que este aviso dava antes da correção não se aplica mais.',
            'Situação: corrigido em stabileo.com em 21 de outubro de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-006-mirrored-moments-2d-in-3d.md`, label: 'SA-2026-006 completo', t: 'O aviso, com os três casos de teste:' },

        { k: 'h', t: 'SA-2026-005: esforço normal em barras inclinadas 2D' },
        { k: 'p', t: 'A montagem de cargas 2D passava aos nós extremos a componente da carga ao longo de uma barra inclinada. Reações e deslocamentos estavam certos, mas a barra não via essa carga no vão, então seu esforço normal ficava constante, igual à média dos valores reais nas extremidades: na base de um caibro, 26 % baixo. O diagrama, o inspetor, o memorial, a exportação para Excel e o dimensionamento usavam esse valor. Além disso, um binário aplicado numa barra se perdia quando um modelo plano era resolvido em 3D.' },
        {
          k: 'ul',
          items: [
            'Como saber se você foi afetado: um modelo 2D resolvido desde 25/09/2026 com barras inclinadas (caibros, escadas, diagonais, arcos por segmentos) com cargas distribuídas, concentradas ou peso próprio com componente ao longo da barra; ou um modelo plano com um binário numa barra resolvido em 3D. As barras dimensionadas à compressão são as mais expostas.',
            'O que fazer: resolva de novo em stabileo.com os modelos afetados e compare com os resultados que você já usou. A alternativa que este aviso dava antes da correção não se aplica mais.',
            'Situação: corrigido em stabileo.com em 21 de outubro de 2026.',
          ],
        },
        { k: 'ext', href: `${REPO}/SA-2026-005-inclined-member-loads.md`, label: 'SA-2026-005 completo', t: 'O aviso, com o caibro e a barra 3-4-5:' },

        { k: 'h', t: 'Corrigidos em 19 de outubro de 2026' },
        {
          k: 'ul',
          items: [
            'SA-2026-009: o elemento sólido-casca SHB8 era 22 % rígido demais na flexão de placas finas (travamento de Poisson na espessura), então subestimava as flechas em cerca de 18 %. Revise as conclusões de flecha e vibração de modelos com placas de sólido-casca resolvidos antes dessa data; MITC4, MITC9, DKT e a casca curva não eram afetados.',
            'SA-2026-010: o certificado da análise espectral podia aprovar resultados não finitos (NaN). Os números eram visivelmente inválidos; o errado era a afirmação de verificação. Não é preciso revisar nada, a menos que um resultado entregue mostrasse valores não finitos.',
          ],
        },

        { k: 'h', t: 'Corrigidos na versão 0.2.0' },
        { k: 'p', t: 'A versão 0.2.0, de 18 de outubro de 2026, corrigiu os quatro avisos retroativos. Todos afetam projetos resolvidos antes de 17 de outubro de 2026, e a recomendação é a mesma: gerar de novo cargas e combinações, resolver e revisar as verificações, e revisar qualquer entrega emitida com uma versão anterior.' },
        {
          k: 'ul',
          items: [
            'SA-2026-001: combinações e cargas geradas CIRSOC 101, 102 e 103 contra a segurança (num exemplo, o esforço normal de um pilar ficava 29 % baixo).',
            'SA-2026-002: verificações de concreto CIRSOC 201 e ACI 318 com capacidade superestimada (φMn até 18 % alto).',
            'SA-2026-003: verificações de aço, perfis formados a frio e ligações com capacidade superestimada (cisalhamento da alma até 60 % alto).',
            'SA-2026-004: análises que ignoravam parte do modelo (o sismo por acelerograma perdia até metade da carga; o colapso plástico 3D, até 2,3 vezes contra a segurança).',
          ],
        },
        { k: 'ext', href: `${REPO}/README.md`, label: 'Todos os avisos', t: 'A lista completa, com cada aviso, sua correção e sua severidade:' },
        { k: 'link', slug: 'how-stabileo-checks-its-results', t: 'Como encontramos esses defeitos, e o que nenhuma verificação pode garantir:' },
      ],
    },
  },
};
