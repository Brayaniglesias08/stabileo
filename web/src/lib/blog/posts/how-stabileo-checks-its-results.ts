/**
 * How the results are checked, and what the checks do not cover.
 *
 * Every claim here has a source in the repository and must stay no stronger
 * than it:
 *  - the trusted base and what Lean proves: docs/verification/TRUSTED_BASE.md
 *    (assembly of K and f is NOT certified; Rust is not proved to match the
 *    Lean algorithm except the bundle checkers, which are Lean code; design
 *    formulas are not formalized; 37 top-level theorems are axiom-checked);
 *  - the acceptance suite: docs/verification/ACCEPTANCE.md (12 cases, 105
 *    published values, mostly frequencies of building frames);
 *  - the seven differential programs: tools/differential, tools/opensees;
 *  - the AI boundary: the-determinism-boundary post and web/src/lib/ai
 *    (the assistant calls a server; the solver does not).
 * If one of those changes, this post changes with it. "Certified" is never
 * used for more than "a machine-checked bound on the error of solving the
 * assembled system".
 */
import type { Post } from '../types';

const REPO = 'https://github.com/lambdaclass/stabileo/blob/main';

export const howStabileoChecksItsResults: Post = {
  slug: 'how-stabileo-checks-its-results',
  date: '2026-10-20',
  order: 7,
  authors: ['Federico Carrone'],
  tagKeys: ['blog.tag.verification', 'blog.tag.solver', 'blog.tag.ai'],
  i18n: {
    es: {
      title: 'Cómo verificamos los resultados de Stabileo, y cuáles son sus límites',
      excerpt:
        'El solver, la verificación formal y la IA: cómo buscamos errores en los números que da Stabileo, qué prueban los certificados y qué no puede garantizar ninguna verificación.',
      blocks: [
        { k: 'p', t: 'Un programa de cálculo estructural puede dar un número equivocado sin ninguna señal visible. En las últimas semanas pusimos a prueba cada parte de Stabileo de varias maneras independientes, encontramos decenas de errores reales, muchos del lado inseguro, y corregimos la mayoría; los abiertos que pueden afectar resultados ya usados están publicados como avisos, con qué hacer mientras tanto. Este artículo cuenta cómo buscamos esos errores, qué demuestran los certificados que acompañan a los resultados y, con la misma claridad, qué queda fuera de toda verificación.' },

        { k: 'h', t: 'Un solo motor de cálculo' },
        { k: 'p', t: 'El solver está escrito en Rust y se compila a WebAssembly: corre en la computadora de quien lo usa, sin instalación y sin enviar el modelo a ningún servidor. Es el mismo código que se prueba en la integración continua. En octubre de 2026 quitamos los cálculos paralelos en TypeScript que reemplazaban en silencio al motor cuando éste no estaba disponible: ahora la aplicación lo dice, en lugar de calcular con una copia no verificada.' },

        { k: 'h', t: 'Cinco formas de buscar errores' },
        { k: 'p', t: 'Ninguna prueba encuentra todos los errores, así que usamos varias que fallan de maneras distintas:' },
        {
          k: 'ul',
          items: [
            'Comparación con otros programas. Los mismos modelos, fijos y generados al azar cada noche, se resuelven con OpenSees, Code_Aster, Frame3DD, PyNite, MYSTRAN, Kratos y CalculiX. Cada diferencia se investiga hasta saber si el error es nuestro o si los dos programas modelan distinto.',
            'Referencias publicadas. Un conjunto de casos de aceptación con resultados publicados por sus autores, no por otro programa comercial, con tolerancias fijadas por la precisión impresa en la fuente. Las verificaciones de diseño se comparan con ejemplos resueltos publicados (INTI-CIRSOC, AISC y otros).',
            'Invariantes físicos. Rotar, reflejar o renumerar un modelo, o cambiar sus unidades, tiene que cambiar la respuesta exactamente como dice la física; la superposición, la reciprocidad y el balance de energía tienen que cumplirse. Estas pruebas no necesitan ningún valor de referencia.',
            'Derivaciones. Las matrices de las barras se derivan simbólicamente desde los principios y se comparan término a término con las del código; para los demás elementos hay derivaciones escritas para que otro ingeniero pueda revisarlas.',
            'Fuzzing y pruebas de mutación. Millones de entradas generadas automáticamente buscan caídas y resultados no finitos, y se introducen errores a propósito en el código para comprobar que las pruebas los detectan.',
          ],
        },
        {
          k: 'table',
          caption: 'Algunas cifras de la verificación, según los documentos del repositorio.',
          head: ['Qué', 'Cantidad'],
          rows: [
            ['Programas de elementos finitos independientes en la comparación', '7'],
            ['Casos de aceptación con resultados publicados', '12'],
            ['Valores de referencia publicados en esos casos', '105'],
            ['Teoremas de Lean cuyos axiomas revisa la integración continua', '37'],
          ],
        },
        { k: 'note', t: 'Los 105 valores de referencia son en su mayoría frecuencias de pórticos de edificios (SAC, IASC-ASCE, Bathe y Wilson). Cubren bien el análisis modal y lineal de pórticos; no son evidencia para otros tipos de análisis.' },

        { k: 'h', t: 'Certificados: un resultado con cota de error' },
        { k: 'p', t: 'Para los resultados que lo admiten, Stabileo intenta además, en segundo plano, un certificado: una cota, calculada con aritmética de intervalos, que contiene la solución exacta del sistema que resolvió. Si no lo logra, el resultado queda marcado como no certificado, con el motivo. La cota es del error numérico, no del error de discretización de la malla o del paso de tiempo. Para el análisis lineal, las combinaciones, el modal y el colapso plástico, la matemática central de esa cota está demostrada en Lean, un asistente de demostración en el que cada paso lo verifica la computadora; algunos pasos (las restricciones, una condición del modal) todavía no están formalizados. El P-Delta, el espectral, el armónico y el dinámico también tienen certificados, pero su matemática todavía no está formalizada en Lean. La integración continua rechaza cualquier demostración incompleta o axioma agregado, y para algunos tipos de certificado el verificador que se ejecuta es el mismo código de Lean sobre el que están las demostraciones.' },

        { k: 'h', t: 'Lo que los certificados no dicen' },
        {
          k: 'ul',
          items: [
            'Un certificado acota el error de resolver el sistema tal como se armó. No prueba que ese sistema represente el modelo: el armado de la matriz de rigidez y de las cargas está probado de las formas de arriba, pero no certificado. Cuando faltaba el pandeo torsional (SA-2026-007), el certificado certificaba el problema equivocado. Los certificados de extremo a extremo, que vuelven a armar el sistema de forma independiente, están en desarrollo.',
            'Lean demuestra la matemática; que el código Rust implemente exactamente ese algoritmo no está demostrado, salvo en los verificadores que son código de Lean.',
            'Las verificaciones certificadas de las normas evalúan sus fórmulas con aritmética de intervalos, pero las fórmulas en sí no están formalizadas: se comparan con ejemplos publicados. Mampostería, uniones, fundaciones y servicio no están certificadas.',
            'Ninguna verificación del programa verifica el modelo. Las cargas, los apoyos y la idealización son decisiones del ingeniero; la revisión del modelo avisa de errores comunes, pero no reemplaza el criterio.',
            'Todavía no hubo una revisión independiente por un ingeniero estructural matriculado. Los resultados de CIRSOC 301 siguen marcados como experimentales.',
          ],
        },

        { k: 'h', t: 'La IA redacta y revisa; no calcula' },
        { k: 'p', t: 'El asistente de IA arma borradores de modelos a partir de una descripción y revisa modelos existentes, pero no calcula resultados: los números del análisis y del diseño los calcula el solver. Sus explicaciones son texto de un modelo de lenguaje y pueden equivocarse; los valores que valen son los de los resultados. Antes de aplicar un cambio se muestra qué va a modificar, y se puede deshacer en un paso. Las funciones de IA envían la descripción y el modelo a un servidor que consulta un modelo de lenguaje; el solver, los resultados y las verificaciones no lo necesitan y funcionan sin conexión.' },
        { k: 'link', slug: 'the-determinism-boundary', t: 'Por qué un agente de IA no debe calcular, con un caso resuelto:' },

        { k: 'h', t: 'Cuando algo falla' },
        { k: 'p', t: 'Los errores que pueden haber afectado resultados ya usados los publicamos como avisos de seguridad, con cómo reconocer un modelo afectado y qué hacer mientras tanto; otros defectos de seguridad están en evaluación y se agregan cuando corresponde. Los avisos abiertos se cierran cuando la corrección está publicada en stabileo.com.' },
        { k: 'link', slug: 'safety-advisories', t: 'Los avisos publicados, con su estado:' },
        { k: 'ext', href: `${REPO}/docs/verification/TRUSTED_BASE.md`, label: 'Base de confianza de los certificados', t: 'Qué está demostrado, qué está probado y qué se confía, componente por componente:' },
      ],
    },
    en: {
      title: 'How we check Stabileo’s results, and where the checks stop',
      excerpt:
        'The solver, formal verification and AI: how we look for errors in the numbers Stabileo gives, what the certificates prove and what no verification can guarantee.',
      blocks: [
        { k: 'p', t: 'A structural analysis program can give a wrong number with no visible sign. Over the last weeks we tested every part of Stabileo in several independent ways, found dozens of real defects, many of them unconservative, and fixed most of them; the open ones that may affect results already in use are published as advisories, with what to do in the meantime. This post explains how we look for those defects, what the certificates that come with the results prove and, just as plainly, what lies outside any verification.' },

        { k: 'h', t: 'One engine' },
        { k: 'p', t: 'The solver is written in Rust and compiled to WebAssembly: it runs on the user’s own computer, with no installation and without sending the model to any server. It is the same code that runs in continuous integration. In October 2026 we removed the parallel TypeScript calculations that silently replaced the engine when it was unavailable: the application now says so, instead of computing with an unverified copy.' },

        { k: 'h', t: 'Five ways of looking for errors' },
        { k: 'p', t: 'No single test finds every error, so we use several that fail in different ways:' },
        {
          k: 'ul',
          items: [
            'Comparison with other programs. The same models, fixed ones and random ones generated every night, are solved with OpenSees, Code_Aster, Frame3DD, PyNite, MYSTRAN, Kratos and CalculiX. Every difference is investigated until we know whether the error is ours or the two programs model the case differently.',
            'Published references. An acceptance suite of cases whose results were published by their authors, not by another commercial program, with tolerances set by the precision printed in the source. Design checks are compared with published worked examples (INTI-CIRSOC, AISC and others).',
            'Physical invariants. Rotating, mirroring or renumbering a model, or changing its units, must change the answer exactly as physics says; superposition, reciprocity and the energy balance must hold. These tests need no reference value at all.',
            'Derivations. The frame element matrices are derived symbolically from first principles and compared term by term with the code’s; for the other elements there are written derivations for another engineer to review.',
            'Fuzzing and mutation testing. Millions of generated inputs look for crashes and non-finite results, and errors are introduced into the code on purpose to check that the tests catch them.',
          ],
        },
        {
          k: 'table',
          caption: 'Some figures of the verification work, from the documents in the repository.',
          head: ['What', 'Count'],
          rows: [
            ['Independent finite-element programs in the comparison', '7'],
            ['Acceptance cases with published results', '12'],
            ['Published reference values in those cases', '105'],
            ['Lean theorems whose axioms continuous integration checks', '37'],
          ],
        },
        { k: 'note', t: 'The 105 reference values are mostly frequencies of building frames (SAC, IASC-ASCE, Bathe and Wilson). They cover modal and linear analysis of frames well; they are not evidence for other kinds of analysis.' },

        { k: 'h', t: 'Certificates: a result with an error bound' },
        { k: 'p', t: 'For results that support it, Stabileo also attempts a certificate in the background: a bound, computed with interval arithmetic, that contains the exact solution of the system it solved. When it cannot, the result is marked uncertified, with the reason. The bound is on numerical error, not on the discretization error of the mesh or the time step. For linear analysis, combinations, modal and plastic collapse, the core mathematics of that bound is proved in Lean, a proof assistant in which the computer checks every step; some steps (constraints, one condition of the modal check) are not yet formalized. P-Delta, spectral, harmonic and time-history analyses have certificates too, but their mathematics is not yet formalized in Lean. Continuous integration rejects any incomplete proof or added axiom, and for some kinds of certificate the checker that runs is the very Lean code the proofs are about.' },

        { k: 'h', t: 'What the certificates do not say' },
        {
          k: 'ul',
          items: [
            'A certificate bounds the error of solving the system as it was assembled. It does not prove that the system represents the model: the assembly of the stiffness matrix and the loads is tested in the ways above, but not certified. When torsional buckling was missing (SA-2026-007), the certificate certified the wrong problem. End-to-end certificates, which re-assemble the system independently, are in progress.',
            'Lean proves the mathematics; that the Rust code implements exactly that algorithm is not proved, except for the checkers that are Lean code.',
            'The certified design checks evaluate their formulas in interval arithmetic, but the formulas themselves are not formalized: they are compared with published examples. Masonry, connections, foundations and serviceability are not certified.',
            'No verification of the program verifies the model. Loads, supports and idealisation are the engineer’s decisions; the model review warns about common mistakes, but it does not replace judgement.',
            'There has not yet been an independent review by a licensed structural engineer. CIRSOC 301 results are still labelled experimental.',
          ],
        },

        { k: 'h', t: 'AI drafts and reviews; it does not calculate' },
        { k: 'p', t: 'The AI assistant drafts models from a description and reviews existing ones, but it does not compute results: the analysis and design numbers come from the solver. Its explanations are language-model text and can be wrong; the values that count are the ones in the results. Before a change is applied it shows what it will modify, and it can be undone in one step. The AI features send the description and the model to a server that queries a language model; the solver, the results and the checks do not need it and work offline.' },
        { k: 'link', slug: 'the-determinism-boundary', t: 'Why an AI agent should not calculate, with a worked case:' },

        { k: 'h', t: 'When something fails' },
        { k: 'p', t: 'Defects that may have affected results already in use are published as safety advisories, with how to recognise an affected model and what to do in the meantime; other safety defects are being triaged and are added when they warrant one. Open advisories are closed when the fix is live on stabileo.com.' },
        { k: 'link', slug: 'safety-advisories', t: 'The published advisories and their status:' },
        { k: 'ext', href: `${REPO}/docs/verification/TRUSTED_BASE.md`, label: 'Trusted base of the certificates', t: 'What is proved, what is tested and what is trusted, component by component:' },
      ],
    },
    pt: {
      title: 'Como verificamos os resultados do Stabileo, e quais são os limites',
      excerpt:
        'O solver, a verificação formal e a IA: como procuramos erros nos números que o Stabileo dá, o que os certificados provam e o que nenhuma verificação pode garantir.',
      blocks: [
        { k: 'p', t: 'Um programa de cálculo estrutural pode dar um número errado sem nenhum sinal visível. Nas últimas semanas testamos cada parte do Stabileo de várias maneiras independentes, encontramos dezenas de defeitos reais, muitos contra a segurança, e corrigimos a maioria; os abertos que podem afetar resultados já usados estão publicados como avisos, com o que fazer enquanto isso. Este texto conta como procuramos esses defeitos, o que provam os certificados que acompanham os resultados e, com a mesma clareza, o que fica fora de qualquer verificação.' },

        { k: 'h', t: 'Um só motor de cálculo' },
        { k: 'p', t: 'O solver é escrito em Rust e compilado para WebAssembly: roda no computador de quem o usa, sem instalação e sem enviar o modelo a nenhum servidor. É o mesmo código testado na integração contínua. Em outubro de 2026 removemos os cálculos paralelos em TypeScript que substituíam o motor em silêncio quando ele não estava disponível: agora a aplicação avisa, em vez de calcular com uma cópia não verificada.' },

        { k: 'h', t: 'Cinco maneiras de procurar erros' },
        { k: 'p', t: 'Nenhum teste encontra todos os erros, então usamos vários que falham de maneiras diferentes:' },
        {
          k: 'ul',
          items: [
            'Comparação com outros programas. Os mesmos modelos, fixos e gerados ao acaso toda noite, são resolvidos com OpenSees, Code_Aster, Frame3DD, PyNite, MYSTRAN, Kratos e CalculiX. Cada diferença é investigada até saber se o erro é nosso ou se os dois programas modelam o caso de forma diferente.',
            'Referências publicadas. Um conjunto de casos de aceitação com resultados publicados pelos autores, não por outro programa comercial, com tolerâncias definidas pela precisão impressa na fonte. As verificações de projeto são comparadas com exemplos resolvidos publicados (INTI-CIRSOC, AISC e outros).',
            'Invariantes físicos. Girar, espelhar ou renumerar um modelo, ou trocar suas unidades, tem que mudar a resposta exatamente como diz a física; a superposição, a reciprocidade e o balanço de energia têm que valer. Esses testes não precisam de nenhum valor de referência.',
            'Derivações. As matrizes das barras são derivadas simbolicamente a partir dos princípios e comparadas termo a termo com as do código; para os demais elementos há derivações escritas para que outro engenheiro possa revisá-las.',
            'Fuzzing e testes de mutação. Milhões de entradas geradas automaticamente procuram falhas e resultados não finitos, e erros são introduzidos de propósito no código para conferir que os testes os detectam.',
          ],
        },
        {
          k: 'table',
          caption: 'Alguns números da verificação, segundo os documentos do repositório.',
          head: ['O quê', 'Quantidade'],
          rows: [
            ['Programas de elementos finitos independentes na comparação', '7'],
            ['Casos de aceitação com resultados publicados', '12'],
            ['Valores de referência publicados nesses casos', '105'],
            ['Teoremas de Lean cujos axiomas a integração contínua confere', '37'],
          ],
        },
        { k: 'note', t: 'Os 105 valores de referência são em sua maioria frequências de pórticos de edifícios (SAC, IASC-ASCE, Bathe e Wilson). Cobrem bem a análise modal e linear de pórticos; não são evidência para outros tipos de análise.' },

        { k: 'h', t: 'Certificados: um resultado com limite de erro' },
        { k: 'p', t: 'Para os resultados que o admitem, o Stabileo tenta também, em segundo plano, um certificado: um limite, calculado com aritmética de intervalos, que contém a solução exata do sistema que resolveu. Se não consegue, o resultado fica marcado como não certificado, com o motivo. O limite é do erro numérico, não do erro de discretização da malha ou do passo de tempo. Para a análise linear, as combinações, a modal e o colapso plástico, a matemática central desse limite está demonstrada em Lean, um assistente de provas em que o computador confere cada passo; alguns passos (as restrições, uma condição da modal) ainda não estão formalizados. O P-Delta, a espectral, a harmônica e a dinâmica também têm certificados, mas sua matemática ainda não está formalizada em Lean. A integração contínua rejeita qualquer prova incompleta ou axioma acrescentado, e para alguns tipos de certificado o verificador que roda é o próprio código Lean sobre o qual estão as provas.' },

        { k: 'h', t: 'O que os certificados não dizem' },
        {
          k: 'ul',
          items: [
            'Um certificado limita o erro de resolver o sistema tal como foi montado. Não prova que esse sistema represente o modelo: a montagem da matriz de rigidez e das cargas é testada das formas acima, mas não certificada. Quando faltava a flambagem torcional (SA-2026-007), o certificado certificava o problema errado. Os certificados de ponta a ponta, que remontam o sistema de forma independente, estão em desenvolvimento.',
            'Lean demonstra a matemática; que o código Rust implemente exatamente esse algoritmo não está demonstrado, exceto nos verificadores que são código Lean.',
            'As verificações certificadas das normas avaliam suas fórmulas com aritmética de intervalos, mas as fórmulas em si não estão formalizadas: são comparadas com exemplos publicados. Alvenaria, ligações, fundações e serviço não são certificadas.',
            'Nenhuma verificação do programa verifica o modelo. As cargas, os apoios e a idealização são decisões do engenheiro; a revisão do modelo avisa de erros comuns, mas não substitui o critério.',
            'Ainda não houve uma revisão independente por um engenheiro estrutural habilitado. Os resultados de CIRSOC 301 continuam marcados como experimentais.',
          ],
        },

        { k: 'h', t: 'A IA redige e revisa; não calcula' },
        { k: 'p', t: 'O assistente de IA monta rascunhos de modelos a partir de uma descrição e revisa modelos existentes, mas não calcula resultados: os números da análise e do projeto vêm do solver. Suas explicações são texto de um modelo de linguagem e podem errar; os valores que valem são os dos resultados. Antes de aplicar uma mudança, ele mostra o que vai modificar, e ela pode ser desfeita num passo. As funções de IA enviam a descrição e o modelo a um servidor que consulta um modelo de linguagem; o solver, os resultados e as verificações não precisam dele e funcionam sem conexão.' },
        { k: 'link', slug: 'the-determinism-boundary', t: 'Por que um agente de IA não deve calcular, com um caso resolvido:' },

        { k: 'h', t: 'Quando algo falha' },
        { k: 'p', t: 'Os defeitos que podem ter afetado resultados já usados são publicados como avisos de segurança, com como reconhecer um modelo afetado e o que fazer enquanto isso; outros defeitos de segurança estão em avaliação e são acrescentados quando cabe. Os avisos abertos são fechados quando a correção está publicada em stabileo.com.' },
        { k: 'link', slug: 'safety-advisories', t: 'Os avisos publicados e sua situação:' },
        { k: 'ext', href: `${REPO}/docs/verification/TRUSTED_BASE.md`, label: 'Base de confiança dos certificados', t: 'O que está demonstrado, o que está testado e o que é confiado, componente por componente:' },
      ],
    },
  },
};
