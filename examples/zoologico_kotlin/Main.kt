fun main(args: Array<String>) {
    val nombre = if (args.isNotEmpty()) args[0] else "visitante"
    saludar(nombre)

    val cuidador = Cuidador("Dra. Ruiz")
    val perro = Perro("Rex", 4, cuidador)
    val gato = Gato("Miau", 2)

    cuidador.adoptar(perro)
    cuidador.adoptar(gato)
    cuidador.ronda()

    for (animal in cuidador.animales) {
        println(describir(animal))
    }

    println("Argumentos recibidos: ${args.joinToString(" ")}")
}
